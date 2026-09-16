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
import dev.pepotech.pepomote.control.ButtonDelivery
import dev.pepotech.pepomote.control.ButtonState
import dev.pepotech.pepomote.net.PmpCodec
import java.util.concurrent.ConcurrentLinkedQueue

/** Qué emula este móvil: decide qué lleva cada paquete INPUT. */
enum class SenderKind {
    /** Wiimote (puntero / Dolphin): 72 bytes, sin stick. */
    WIIMOTE,

    /** Nunchuk: 72 bytes, stick en los bytes 6-7 (FLAG_STICK_VALID). */
    NUNCHUK,

    /**
     * Mando de Wii con Nunchuk en el mismo móvil, apaisado: 72 bytes con el
     * stick (FLAG_STICK_VALID) y los botones del mando y del Nunchuk (C/Z)
     * a la vez, y los sensores remapeados al marco apaisado como el GamePad
     * (el puntero de Dolphin sigue apuntando con el móvil de lado).
     */
    WII_NUNCHUK,

    /**
     * GamePad / Pro Controller de Wii U (Cemu): 80 bytes con el bloque de
     * extensión (stick derecho + táctil) y los sensores remapeados al marco
     * del mando apaisado ([Frame]).
     */
    GAMEPAD,

    /** Switch: the same extension with both sticks and Capture, without a touch surface. */
    SWITCH
}

/**
 * INPUT sigue al giroscopio hasta 250 Hz. Un reloj de 100 Hz mantiene los
 * controles activos cuando faltan sensores, fallan o dejan de dar muestras.
 * Los cambios de botón piden el siguiente envío disponible sin esperar al reloj.
 * Todo comparte hilo y límite de frecuencia; stop espera cualquier envío en curso.
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
     * Rotación de la pantalla (`Surface.ROTATION_*`): decide el remapeo de
     * los sensores (contrato §4). La fija la pantalla GamePad, el mando +
     * Nunchuk y el mando de lado (NES, con [dev.pepotech.pepomote.service.Route.sidewaysRotation]);
     * en vertical vale ROTATION_0 y no cambia nada.
     */
    @Volatile
    var rotation: Int = Surface.ROTATION_0

    /**
     * Apuntado por inclinación (INPUT flags bit4): el receptor saca el cursor
     * del acelerómetro en vez del giroscopio ([MotionSource]). Lo fija el
     * servicio (ajuste × capacidad del receptor) y Ajustes lo cambia con el
     * enlace vivo. Los sensores se envían exactamente igual.
     */
    @Volatile
    var tilt: Boolean = false

    private val sensorManager =
        context.getSystemService(Context.SENSOR_SERVICE) as SensorManager
    private val batteryManager =
        context.getSystemService(Context.BATTERY_SERVICE) as BatteryManager

    private val thread = HandlerThread("pepomote-sensors").apply { start() }
    private val handler = Handler(thread.looper)

    private val quat = floatArrayOf(1f, 0f, 0f, 0f) // w, x, y, z
    private val gyro = FloatArray(3)
    private val latestGyro = FloatArray(3)
    private val accel = FloatArray(3)

    // Sensores ya remapeados al marco del GamePad (sin reservar memoria por paquete)
    private val quatOut = FloatArray(4)
    private val gyroOut = FloatArray(3)
    private val accelOut = FloatArray(3)

    private var seq = 0
    private var running = false
    private var closed = false
    private var gyroRegistered = false
    private var accelRegistered = false
    private var rotationRegistered = false
    private var hasRotationVector = false
    /** Reloj real compartido por muestras y respaldo: una ráfaga de callbacks no acelera INPUT. */
    private var lastSendNs = 0L
    private val buttonChanges = ConcurrentLinkedQueue<ButtonState.Change>()
    private val buttonDelivery = ButtonDelivery()
    /** Media ponderada por tiempo del gyro entre envíos (∑ω·Δt y ∑Δt). */
    private val gyroSum = FloatArray(3)
    private var gyroSumNs = 0L
    private var lastGyroNs = 0L

    private var batteryPct = 100
    private var batteryReadAtMs = 0L

    @Volatile
    var lastSensorHz = 0f
        private set
    private var hzWindowStartNs = 0L
    private var hzCount = 0

    private val scratch = FloatArray(4)

    private val buttonUpdate = object : Runnable {
        override fun run() = synchronized(this@MotionEngine) {
            // Coalesce wake-ups, retaining each bit's pending presses/releases.
            handler.removeCallbacks(this)
            if (!running) return@synchronized
            acceptButtonChanges()
            val nextChange = buttonDelivery.nextChangeAtNs() ?: return@synchronized
            val now = SystemClock.elapsedRealtimeNanos()
            val remaining = maxOf(lastSendNs + GYRO_INTERVAL_NS, nextChange) - now
            if (remaining > 0) {
                handler.postDelayed(this, (remaining + 999_999L) / 1_000_000L)
                return@synchronized
            }
            prepareGyro(now)
            lastSendNs = now
            sendPacket(now)
        }
    }

    private val onButtonChange: (ButtonState.Change) -> Unit = { change ->
        // Notifications may hold the input-state/latch locks: enqueue only,
        // without taking the engine lock or doing network work on the UI thread.
        if (change.reset) buttonChanges.clear()
        buttonChanges.add(change)
        handler.post(buttonUpdate)
    }

    private val fallback = object : Runnable {
        override fun run() = synchronized(this@MotionEngine) {
            if (!running) return@synchronized
            val now = SystemClock.elapsedRealtimeNanos()
            if (now - lastSendNs >= FALLBACK_INTERVAL_NS) {
                prepareGyro(now)
                lastSendNs = now
                sendPacket(now)
            }
            if (running) handler.postDelayed(this, FALLBACK_INTERVAL_MS)
        }
    }

    @Synchronized
    fun start() {
        if (running || closed) return
        running = true
        ButtonState.addButtonChangeListener(onButtonChange)
        gyroRegistered = register(Sensor.TYPE_GYROSCOPE)
        accelRegistered = register(Sensor.TYPE_ACCELEROMETER)
        rotationRegistered = register(Sensor.TYPE_GAME_ROTATION_VECTOR)
        handler.postDelayed(fallback, FALLBACK_INTERVAL_MS)
    }

    private fun register(type: Int): Boolean = try {
        val sensor = sensorManager.getDefaultSensor(type)
        sensor != null && sensorManager.registerListener(this, sensor, SensorManager.SENSOR_DELAY_FASTEST, handler)
    } catch (_: RuntimeException) {
        // Algunos dispositivos exponen el sensor pero rechazan su registro.
        false
    }

    @Synchronized
    fun stop() {
        if (closed) return
        running = false
        closed = true
        ButtonState.removeButtonChangeListener(onButtonChange)
        handler.removeCallbacksAndMessages(null)
        buttonChanges.clear()
        sensorManager.unregisterListener(this)
        thread.quitSafely()
    }

    @Synchronized
    override fun onSensorChanged(event: SensorEvent) {
        if (!running) return
        when (event.sensor.type) {
            Sensor.TYPE_GAME_ROTATION_VECTOR -> {
                if (!rotationRegistered) return
                SensorManager.getQuaternionFromVector(scratch, event.values)
                // getQuaternionFromVector devuelve [w, x, y, z]
                quat[0] = scratch[0]; quat[1] = scratch[1]
                quat[2] = scratch[2]; quat[3] = scratch[3]
                hasRotationVector = true
            }

            Sensor.TYPE_ACCELEROMETER -> {
                if (!accelRegistered) return
                accel[0] = event.values[0]
                accel[1] = event.values[1]
                accel[2] = event.values[2]
            }

            Sensor.TYPE_GYROSCOPE -> {
                if (!gyroRegistered) return
                val now = SystemClock.elapsedRealtimeNanos()
                val age = now - event.timestamp
                if (age !in 0 until GYRO_STALE_NS || lastGyroNs != 0L && event.timestamp <= lastGyroNs) return
                trackHz(event.timestamp)
                // Media ponderada por tiempo de las muestras entre envíos: un
                // gyro a 400-500 Hz decimado a 250 Hz sin promediar tiraba
                // muestras (y el receptor integra la que llega sobre todo el
                // intervalo). Con un hueco raro se reinicia.
                val gap = event.timestamp - lastGyroNs
                if (lastGyroNs != 0L && gap in 1..50_000_000L) {
                    for (i in 0..2) gyroSum[i] += event.values[i] * gap
                    gyroSumNs += gap
                } else {
                    gyroSum.fill(0f); gyroSumNs = 0L
                }
                lastGyroNs = event.timestamp
                for (i in 0..2) latestGyro[i] = event.values[i]
                // Tope de 250 Hz del protocolo: móviles con gyro a 400-500 Hz
                // saturan el Wi-Fi y provocan ráfagas/pérdidas (cursor errático)
                if (now - lastSendNs >= GYRO_INTERVAL_NS) {
                    lastSendNs = now
                    consumeGyro()
                    sendPacket(now)
                }
            }
        }
    }

    private fun consumeGyro() {
        for (i in 0..2) gyro[i] = if (gyroSumNs > 0L) gyroSum[i] / gyroSumNs else latestGyro[i]
        gyroSum.fill(0f)
        gyroSumNs = 0L
    }

    private fun prepareGyro(now: Long) {
        if (lastGyroNs != 0L && now - lastGyroNs < GYRO_STALE_NS) {
            // Un gyro de 50 Hz sigue vivo entre envíos; conservar también
            // muestras que llegaron después del anterior INPUT.
            consumeGyro()
        } else {
            // Una pulsación no debe reactivar el movimiento de un sensor parado.
            gyro.fill(0f)
            gyroSum.fill(0f)
            gyroSumNs = 0L
            lastGyroNs = 0L
            lastSensorHz = 0f
            hzWindowStartNs = 0L
            hzCount = 0
        }
    }

    private fun acceptButtonChanges() {
        while (true) {
            val change = buttonChanges.poll() ?: return
            if (change.reset) buttonDelivery.reset() else buttonDelivery.accept(change.buttons)
        }
    }

    private fun scheduleButtonUpdate() {
        handler.removeCallbacks(buttonUpdate)
        if (!running) return
        // Remove wake-ups before draining. A concurrent new event either gets
        // drained here or leaves its own wake-up queued after this one.
        acceptButtonChanges()
        val nextChange = buttonDelivery.nextChangeAtNs() ?: return
        val now = SystemClock.elapsedRealtimeNanos()
        val remaining = (maxOf(lastSendNs + GYRO_INTERVAL_NS, nextChange) - now).coerceAtLeast(0)
        handler.postDelayed(buttonUpdate, (remaining + 999_999L) / 1_000_000L)
    }

    private fun sendPacket(tSensorNs: Long) {
        acceptButtonChanges()
        val buttons = buttonDelivery.buttonsForPacket(tSensorNs)
        seq++
        // Los sensores van siempre igual; la inclinación solo añade su bit
        val quatFlag = (if (hasRotationVector) PmpCodec.FLAG_QUAT_VALID else 0) or
            (if (tilt) PmpCodec.FLAG_TILT else 0)
        val packet = when (val senderKind = kind) {
            SenderKind.GAMEPAD, SenderKind.SWITCH -> {
                // Marco del mando apaisado (contrato §4) ANTES de escribir el paquete
                val rot = rotation
                Frame.remapQuat(quat, if (hasRotationVector) rot else Surface.ROTATION_0, quatOut)
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
                    buttons = buttons,
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
                    touchY = touch.y,
                    switchPad = senderKind == SenderKind.SWITCH
                )
            }

            SenderKind.NUNCHUK -> PmpCodec.encodeInput(
                sessionId = sessionId,
                seq = seq,
                tSensorUs = tSensorNs / 1000,
                quat = quat,
                gyro = gyro,
                accel = accel,
                buttons = buttons,
                recenterCount = ButtonState.recenterCount(),
                batteryPct = battery(),
                touchScrollDy = ButtonState.drainScroll(),
                flags = quatFlag or PmpCodec.FLAG_STICK_VALID,
                stickX = ButtonState.stickX(),
                stickY = ButtonState.stickY()
            )

            SenderKind.WII_NUNCHUK -> {
                // Como el Nunchuk (stick en la trama) pero con el móvil de lado:
                // sensores al marco apaisado (contrato §4), igual que el GamePad
                val rot = rotation
                Frame.remapQuat(quat, if (hasRotationVector) rot else Surface.ROTATION_0, quatOut)
                Frame.remapGyro(gyro, rot, gyroOut)
                Frame.remapAccel(accel, rot, accelOut)
                PmpCodec.encodeInput(
                    sessionId = sessionId,
                    seq = seq,
                    tSensorUs = tSensorNs / 1000,
                    quat = quatOut,
                    gyro = gyroOut,
                    accel = accelOut,
                    buttons = buttons,
                    recenterCount = ButtonState.recenterCount(),
                    batteryPct = battery(),
                    touchScrollDy = ButtonState.drainScroll(),
                    flags = quatFlag or PmpCodec.FLAG_STICK_VALID,
                    stickX = ButtonState.stickX(),
                    stickY = ButtonState.stickY()
                )
            }

            SenderKind.WIIMOTE -> {
                // Móvil de lado (NES): sensores girados según diga la pantalla
                // (Route.sidewaysRotation); en vertical (ROTATION_0), tal cual
                val rot = rotation
                Frame.remapQuat(quat, if (hasRotationVector) rot else Surface.ROTATION_0, quatOut)
                Frame.remapGyro(gyro, rot, gyroOut)
                Frame.remapAccel(accel, rot, accelOut)
                PmpCodec.encodeInput(
                    sessionId = sessionId,
                    seq = seq,
                    tSensorUs = tSensorNs / 1000,
                    quat = quatOut,
                    gyro = gyroOut,
                    accel = accelOut,
                    buttons = buttons,
                    recenterCount = ButtonState.recenterCount(),
                    batteryPct = battery(),
                    touchScrollDy = ButtonState.drainScroll(),
                    flags = quatFlag
                )
            }
        }
        onPacket(packet)
        // Sending/encoding can itself be delayed. Neither another packet nor
        // a queued release may shorten a pulse by using the earlier enqueue time.
        lastSendNs = SystemClock.elapsedRealtimeNanos()
        buttonDelivery.packetSent(buttons, lastSendNs)
        scheduleButtonUpdate()
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

    private companion object {
        const val GYRO_INTERVAL_NS = 4_000_000L
        const val GYRO_STALE_NS = 50_000_000L
        const val FALLBACK_INTERVAL_MS = 10L
        const val FALLBACK_INTERVAL_NS = FALLBACK_INTERVAL_MS * 1_000_000L
    }
}
