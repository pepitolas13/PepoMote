package dev.pepotech.pepomote.sensor

import android.content.Context
import android.hardware.Sensor
import android.hardware.SensorEvent
import android.hardware.SensorEventListener
import android.hardware.SensorManager
import android.os.BatteryManager
import android.os.Handler
import android.os.HandlerThread
import android.os.SystemClock
import android.view.Surface
import dev.pepotech.pepomote.control.ButtonState
import dev.pepotech.pepomote.net.PmpCodec

/** Qué emula este móvil: decide qué lleva cada paquete INPUT. */
enum class SenderKind {
    /** Wiimote (puntero / Dolphin): 72 bytes, sin stick. */
    WIIMOTE,

    /** Nunchuk: 72 bytes, stick en los bytes 6-7 (FLAG_STICK_VALID). */
    NUNCHUK,

    /**
     * GamePad / Pro Controller de Wii U (Cemu): 80 bytes con el bloque de
     * extensión (stick derecho + táctil) y los sensores remapeados al marco
     * del mando apaisado ([Frame]).
     */
    GAMEPAD
}

/**
 * Sensores a máxima frecuencia. La cadencia de envío la marca el gyro:
 * un paquete INPUT por muestra de gyro (tope natural del hardware).
 */
class MotionEngine(
    context: Context,
    private val sessionId: Int,
    kind: SenderKind = SenderKind.WIIMOTE,
    private val onPacket: (ByteArray) -> Unit
) : SensorEventListener {

    /**
     * Qué se emula. El servicio lo fija al conectar (Wiimote o Nunchuk según
     * el rol); la pantalla GamePad lo pone en GAMEPAD al entrar y lo restaura
     * al salir (y con él vuelven los paquetes de 72 bytes).
     */
    @Volatile
    var kind: SenderKind = kind

    /**
     * Rotación de la pantalla (`Surface.ROTATION_*`) mientras se es GamePad:
     * decide el remapeo de los sensores (contrato §4). La fija la pantalla
     * GamePad; en cualquier otra pantalla no se usa.
     */
    @Volatile
    var rotation: Int = Surface.ROTATION_0

    private val sensorManager =
        context.getSystemService(Context.SENSOR_SERVICE) as SensorManager
    private val batteryManager =
        context.getSystemService(Context.BATTERY_SERVICE) as BatteryManager

    private val thread = HandlerThread("pepomote-sensors").apply { start() }
    private val handler = Handler(thread.looper)

    private val quat = floatArrayOf(1f, 0f, 0f, 0f) // w, x, y, z
    private val gyro = FloatArray(3)
    private val accel = FloatArray(3)

    // Sensores ya remapeados al marco del GamePad (sin reservar memoria por paquete)
    private val quatOut = FloatArray(4)
    private val gyroOut = FloatArray(3)
    private val accelOut = FloatArray(3)

    private var seq = 0
    private var hasRotationVector = false
    private var lastSendNs = 0L

    private var batteryPct = 100
    private var batteryReadAtMs = 0L

    @Volatile
    var lastSensorHz = 0f
        private set
    private var hzWindowStartNs = 0L
    private var hzCount = 0

    private val scratch = FloatArray(4)

    fun start() {
        val gyroSensor = sensorManager.getDefaultSensor(Sensor.TYPE_GYROSCOPE)
        val accelSensor = sensorManager.getDefaultSensor(Sensor.TYPE_ACCELEROMETER)
        val rotSensor = sensorManager.getDefaultSensor(Sensor.TYPE_GAME_ROTATION_VECTOR)
        hasRotationVector = rotSensor != null

        gyroSensor?.let {
            sensorManager.registerListener(this, it, SensorManager.SENSOR_DELAY_FASTEST, handler)
        }
        accelSensor?.let {
            sensorManager.registerListener(this, it, SensorManager.SENSOR_DELAY_FASTEST, handler)
        }
        rotSensor?.let {
            sensorManager.registerListener(this, it, SensorManager.SENSOR_DELAY_FASTEST, handler)
        }
    }

    fun stop() {
        sensorManager.unregisterListener(this)
        thread.quitSafely()
    }

    override fun onSensorChanged(event: SensorEvent) {
        when (event.sensor.type) {
            Sensor.TYPE_GAME_ROTATION_VECTOR -> {
                SensorManager.getQuaternionFromVector(scratch, event.values)
                // getQuaternionFromVector devuelve [w, x, y, z]
                quat[0] = scratch[0]; quat[1] = scratch[1]
                quat[2] = scratch[2]; quat[3] = scratch[3]
            }

            Sensor.TYPE_ACCELEROMETER -> {
                accel[0] = event.values[0]
                accel[1] = event.values[1]
                accel[2] = event.values[2]
            }

            Sensor.TYPE_GYROSCOPE -> {
                gyro[0] = event.values[0]
                gyro[1] = event.values[1]
                gyro[2] = event.values[2]
                trackHz(event.timestamp)
                // Tope de 250 Hz del protocolo: móviles con gyro a 400-500 Hz
                // saturan el Wi-Fi y provocan ráfagas/pérdidas (cursor errático)
                if (event.timestamp - lastSendNs >= 3_900_000L) {
                    lastSendNs = event.timestamp
                    sendPacket(event.timestamp)
                }
            }
        }
    }

    private fun sendPacket(tSensorNs: Long) {
        seq++
        val quatFlag = if (hasRotationVector) PmpCodec.FLAG_QUAT_VALID else 0
        val packet = when (kind) {
            SenderKind.GAMEPAD -> {
                // Marco del mando apaisado (contrato §4) ANTES de escribir el paquete
                val rot = rotation
                Frame.remapQuat(quat, rot, quatOut)
                Frame.remapGyro(gyro, rot, gyroOut)
                Frame.remapAccel(accel, rot, accelOut)
                val touch = ButtonState.touch()
                PmpCodec.encodeInput(
                    sessionId = sessionId,
                    seq = seq,
                    tSensorUs = tSensorNs / 1000,
                    quat = quatOut,
                    gyro = gyroOut,
                    accel = accelOut,
                    buttons = ButtonState.current(),
                    recenterCount = ButtonState.recenterCount(),
                    batteryPct = battery(),
                    touchScrollDy = ButtonState.drainScroll(),
                    flags = quatFlag or PmpCodec.FLAG_STICK_VALID or PmpCodec.FLAG_EXT or
                        (if (touch.down) PmpCodec.FLAG_TOUCH else 0),
                    stickX = ButtonState.stickX(),
                    stickY = ButtonState.stickY(),
                    stick2X = ButtonState.stickRX(),
                    stick2Y = ButtonState.stickRY(),
                    touchX = touch.x,
                    touchY = touch.y
                )
            }

            SenderKind.NUNCHUK -> PmpCodec.encodeInput(
                sessionId = sessionId,
                seq = seq,
                tSensorUs = tSensorNs / 1000,
                quat = quat,
                gyro = gyro,
                accel = accel,
                buttons = ButtonState.current(),
                recenterCount = ButtonState.recenterCount(),
                batteryPct = battery(),
                touchScrollDy = ButtonState.drainScroll(),
                flags = quatFlag or PmpCodec.FLAG_STICK_VALID,
                stickX = ButtonState.stickX(),
                stickY = ButtonState.stickY()
            )

            SenderKind.WIIMOTE -> PmpCodec.encodeInput(
                sessionId = sessionId,
                seq = seq,
                tSensorUs = tSensorNs / 1000,
                quat = quat,
                gyro = gyro,
                accel = accel,
                buttons = ButtonState.current(),
                recenterCount = ButtonState.recenterCount(),
                batteryPct = battery(),
                touchScrollDy = ButtonState.drainScroll(),
                flags = quatFlag
            )
        }
        onPacket(packet)
    }

    private fun trackHz(tNs: Long) {
        if (hzWindowStartNs == 0L) hzWindowStartNs = tNs
        hzCount++
        val elapsed = tNs - hzWindowStartNs
        if (elapsed > 1_000_000_000L) {
            lastSensorHz = hzCount * 1e9f / elapsed
            hzWindowStartNs = tNs
            hzCount = 0
        }
    }

    private fun battery(): Int {
        val now = SystemClock.elapsedRealtime()
        if (now - batteryReadAtMs > 5000) {
            batteryReadAtMs = now
            val pct = batteryManager.getIntProperty(BatteryManager.BATTERY_PROPERTY_CAPACITY)
            if (pct in 0..100) batteryPct = pct
        }
        return batteryPct
    }

    override fun onAccuracyChanged(sensor: Sensor?, accuracy: Int) = Unit
}
