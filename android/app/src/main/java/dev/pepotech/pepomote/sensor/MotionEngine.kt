package dev.pepotech.pepomote.sensor

import android.content.Context
import android.hardware.Sensor
import android.hardware.SensorEvent
import android.hardware.SensorEventListener
import android.hardware.SensorManager
import android.os.BatteryManager
import android.os.HandlerThread
import android.os.Handler
import android.os.SystemClock
import dev.pepotech.pepomote.control.ButtonState
import dev.pepotech.pepomote.net.PmpCodec

/**
 * Sensores a máxima frecuencia. La cadencia de envío la marca el gyro:
 * un paquete INPUT por muestra de gyro (tope natural del hardware).
 */
class MotionEngine(
    context: Context,
    private val sessionId: Int,
    private val onPacket: (ByteArray) -> Unit
) : SensorEventListener {

    private val sensorManager =
        context.getSystemService(Context.SENSOR_SERVICE) as SensorManager
    private val batteryManager =
        context.getSystemService(Context.BATTERY_SERVICE) as BatteryManager

    private val thread = HandlerThread("pepomote-sensors").apply { start() }
    private val handler = Handler(thread.looper)

    private val quat = floatArrayOf(1f, 0f, 0f, 0f) // w, x, y, z
    private val gyro = FloatArray(3)
    private val accel = FloatArray(3)
    private var seq = 0
    private var hasRotationVector = false

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
                sendPacket(event.timestamp)
            }
        }
    }

    private fun sendPacket(tSensorNs: Long) {
        seq++
        val packet = PmpCodec.encodeInput(
            sessionId = sessionId,
            seq = seq,
            tSensorUs = tSensorNs / 1000,
            quat = quat,
            gyro = gyro,
            accel = accel,
            buttons = ButtonState.current(),
            recenterCount = ButtonState.recenterCount(),
            batteryPct = battery(),
            touchScrollDy = ButtonState.drainScroll()
        )
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
