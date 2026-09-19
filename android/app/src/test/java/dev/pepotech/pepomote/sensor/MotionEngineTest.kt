package dev.pepotech.pepomote.sensor

import android.content.Context
import android.hardware.Sensor
import android.hardware.SensorManager
import android.hardware.SensorEventListener
import android.os.Handler
import android.os.SystemClock
import dev.pepotech.pepomote.control.ButtonState
import dev.pepotech.pepomote.net.PmpCodec
import org.junit.After
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.LooperMode
import org.robolectric.annotation.Implements
import org.robolectric.annotation.Implementation
import org.robolectric.shadows.SensorEventBuilder
import org.robolectric.shadows.ShadowLooper
import org.robolectric.shadows.ShadowSensor
import org.robolectric.shadows.ShadowSensorManager
import org.robolectric.shadows.ShadowSystemClock
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.time.Duration

/** Real engine, ButtonState and PMP bytes, with Android's sensors and clock controlled by Robolectric. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
@LooperMode(LooperMode.Mode.PAUSED)
class MotionEngineTest {
    private val context: Context get() = RuntimeEnvironment.getApplication()
    private lateinit var sensors: ShadowSensorManager
    private val engines = mutableListOf<MotionEngine>()
    private val loopers = mutableMapOf<MotionEngine, ShadowLooper>()

    @Before fun resetInputAndSensors() {
        sensors = shadowOf(context.getSystemService(Context.SENSOR_SERVICE) as SensorManager)
        sensors.getSensorList(Sensor.TYPE_ALL).toList().forEach(sensors::removeSensor)
        ButtonState.reset()
    }

    @After fun stopAll() {
        engines.forEach(MotionEngine::stop)
        ButtonState.reset()
    }

    private fun engine(kind: SenderKind = SenderKind.WIIMOTE, packets: MutableList<ByteArray>, sentAt: MutableList<Long>? = null): MotionEngine {
        val existing = ShadowLooper.getAllLoopers().toSet()
        val result = MotionEngine(context, 42, kind) {
            packets.add(it)
            sentAt?.add(SystemClock.elapsedRealtimeNanos())
        }
        val looper = ShadowLooper.getAllLoopers().single { it !in existing && it.thread.name == "pepomote-sensors" }
        loopers[result] = shadowOf(looper).apply { pause() }
        engines.add(result)
        return result
    }

    private fun stop(engine: MotionEngine) {
        engine.stop()
        loopers.remove(engine)
    }

    private fun advance(ms: Int) {
        repeat(ms) {
            ShadowSystemClock.advanceBy(Duration.ofMillis(1))
            shadowOf(android.os.Looper.getMainLooper()).idle()
            loopers.values.forEach { it.idle() }
        }
    }

    private fun sensor(type: Int): Sensor = ShadowSensor.newInstance(type).also(sensors::addSensor)
    private fun emit(sensor: Sensor, vararg values: Float) {
        sensors.sendSensorEventToListeners(SensorEventBuilder.newBuilder()
            .setSensor(sensor).setTimestamp(SystemClock.elapsedRealtimeNanos()).setValues(values).build(), sensor)
    }

    private fun ByteArray.int(offset: Int) = ByteBuffer.wrap(this).order(ByteOrder.LITTLE_ENDIAN).getInt(offset)
    private fun ByteArray.long(offset: Int) = ByteBuffer.wrap(this).order(ByteOrder.LITTLE_ENDIAN).getLong(offset)
    private fun ByteArray.floats(offset: Int, count: Int) = FloatArray(count) {
        ByteBuffer.wrap(this).order(ByteOrder.LITTLE_ENDIAN).getFloat(offset + it * 4)
    }

    @Test fun elTecladoCongelaLaPoseYAunAsiEntregaElClicEntero() {
        val packets = mutableListOf<ByteArray>()
        val sentAt = mutableListOf<Long>()
        val engine = engine(SenderKind.WIIMOTE, packets, sentAt)
        val gyro = sensor(Sensor.TYPE_GYROSCOPE)
        engine.start()
        // Sin congelar, el giro real viaja
        repeat(10) { advance(5); emit(gyro, 0.5f, 0f, 0f) }
        advance(10)
        assertTrue("el giro real viaja", packets.any { it.floats(40, 3)[0] != 0f })

        // Teclado abierto: el móvil se sigue moviendo en la mano, pero el
        // paquete va quieto — y el clic del botón «Teclado» sale entero
        engine.pointerHold = true
        val desde = packets.size
        ButtonState.set(ButtonState.A, true)
        ButtonState.set(ButtonState.A, false)
        repeat(60) { advance(5); emit(gyro, 0.5f, 0f, 0f) }
        advance(20)
        val congelados = packets.drop(desde)
        assertTrue("sigue emitiendo con el teclado abierto (${'$'}{congelados.size})", congelados.size > 20)
        assertTrue("el giro va a cero", congelados.all { p -> p.floats(40, 3).all { it == 0f } })
        val quat = congelados.first().floats(24, 4)
        assertTrue("el cuaternión no cambia", congelados.all { it.floats(24, 4).contentEquals(quat) })

        // El clic: pulsado, soltado, y con sus 70 ms en el cable (si el envío
        // se cortara entre los dos flancos, el PC se quedaría con el botón
        // izquierdo pulsado para siempre)
        val abajo = congelados.indexOfFirst { it.int(64) and ButtonState.A != 0 }
        val ultimoAbajo = congelados.indexOfLast { it.int(64) and ButtonState.A != 0 }
        assertTrue("el clic se pulsa", abajo >= 0)
        assertTrue("y se suelta antes de acabar", ultimoAbajo in abajo until congelados.size - 1)
        val ms = (sentAt[desde + ultimoAbajo + 1] - sentAt[desde + abajo]) / 1_000_000L
        assertTrue("el clic dura al menos 70 ms en el cable (${'$'}ms)", ms >= 70)
        stop(engine)
    }

    @Test fun alCerrarElTecladoElCursorSigueQuietoUnMomentoYDespuesVuelve() {
        val packets = mutableListOf<ByteArray>()
        val engine = engine(SenderKind.WIIMOTE, packets)
        val gyro = sensor(Sensor.TYPE_GYROSCOPE)
        engine.start()
        engine.pointerHold = true
        repeat(10) { advance(5); emit(gyro, 0.5f, 0f, 0f) }
        engine.pointerHold = false
        // Gracia: el PC tiene que teclear ANTES de que el cursor se mueva
        val desde = packets.size
        repeat(30) { advance(5); emit(gyro, 0.5f, 0f, 0f) }
        assertTrue(
            "durante la gracia el puntero sigue quieto",
            packets.drop(desde).all { p -> p.floats(40, 3).all { it == 0f } }
        )
        // Y pasada la gracia, vuelven los sensores de verdad
        repeat(40) { advance(5); emit(gyro, 0.5f, 0f, 0f) }
        assertTrue("después vuelve el sensor", packets.last().floats(40, 3)[0] != 0f)
        stop(engine)
    }

    @Test fun noSensorsSendRegularInputInEveryControllerFormat() {
        for (kind in SenderKind.entries) {
            val packets = mutableListOf<ByteArray>()
            val engine = engine(kind, packets).apply { rotation = Frame.ROTATION_90; start() }
            advance(100)
            assertEquals("$kind sends at 100 Hz without sensor events", 10, packets.size)
            val extended = kind == SenderKind.GAMEPAD || kind == SenderKind.SWITCH
            packets.forEachIndexed { index, packet ->
                assertEquals(if (extended) 80 else 72, packet.size)
                assertEquals(PmpCodec.TYPE_INPUT, packet[4])
                assertEquals(42, packet.int(8))
                assertEquals(index + 1, packet.int(12))
                assertEquals(0, packet[5].toInt() and PmpCodec.FLAG_QUAT_VALID)
                assertArrayEquals(floatArrayOf(1f, 0f, 0f, 0f), packet.floats(24, 4), 0f)
                assertArrayEquals(FloatArray(3), packet.floats(40, 3), 0f)
                if (index > 0) assertEquals(10_000L, packet.long(16) - packets[index - 1].long(16))
            }
            stop(engine)
        }
    }

    @Test fun noSensorsStillEncodeButtonsSticksTouchAndReleases() {
        val packets = mutableListOf<ByteArray>()
        engine(SenderKind.GAMEPAD, packets).start()
        val buttons = ButtonState.A or ButtonState.B or ButtonState.X or ButtonState.Y or
            ButtonState.L or ButtonState.R or ButtonState.ZL or ButtonState.ZR or
            ButtonState.STICK_L or ButtonState.STICK_R or ButtonState.HOME or ButtonState.PLUS or
            ButtonState.MINUS or ButtonState.DPAD_UP or ButtonState.C or ButtonState.Z or ButtonState.MIC
        ButtonState.set(buttons, true)
        ButtonState.setStick(100, -80)
        ButtonState.setStick2(-64, 32)
        ButtonState.setTouch(12000, 34000, true)
        ButtonState.addScroll(25)
        advance(100)
        assertTrue("Input is sent without any sensor", packets.isNotEmpty())
        val held = packets.first()
        assertEquals(buttons, held.int(64))
        assertEquals(100, held[6].toInt())
        assertEquals(-80, held[7].toInt())
        assertEquals(-64, held[72].toInt())
        assertEquals(32, held[73].toInt())
        assertTrue(held[5].toInt() and PmpCodec.FLAG_TOUCH != 0)
        assertEquals(12000, ByteBuffer.wrap(held).order(ByteOrder.LITTLE_ENDIAN).getShort(74).toInt() and 0xffff)
        assertEquals(34000, ByteBuffer.wrap(held).order(ByteOrder.LITTLE_ENDIAN).getShort(76).toInt() and 0xffff)
        assertEquals(25, ByteBuffer.wrap(held).order(ByteOrder.LITTLE_ENDIAN).getShort(70).toInt())
        ButtonState.set(buttons, false)
        ButtonState.setStick(0, 0)
        ButtonState.setStick2(0, 0)
        ButtonState.setTouch(12000, 34000, false)
        advance(10)
        val released = packets.last()
        assertEquals(0, released.int(64))
        assertEquals(0, released[6].toInt())
        assertEquals(0, released[7].toInt())
        assertEquals(0, released[72].toInt())
        assertEquals(0, released[73].toInt())
        assertEquals(0, released[5].toInt() and PmpCodec.FLAG_TOUCH)
        assertEquals(0, ByteBuffer.wrap(released).order(ByteOrder.LITTLE_ENDIAN).getShort(70).toInt())
    }

    @Test fun accelerometerWorksWithoutGyroOrRotationVector() {
        val accel = sensor(Sensor.TYPE_ACCELEROMETER)
        val packets = mutableListOf<ByteArray>()
        engine(SenderKind.GAMEPAD, packets).apply { rotation = Frame.ROTATION_90; start() }
        emit(accel, 1f, 2f, 9.8f)
        advance(20)
        assertTrue("Accelerometer-only devices send input", packets.isNotEmpty())
        assertArrayEquals(floatArrayOf(-2f, 1f, 9.8f), packets.last().floats(52, 3), 0.0001f)
    }

    @Test fun failedSensorRegistrationStillSendsNeutralInput() {
        sensor(Sensor.TYPE_GYROSCOPE)
        sensor(Sensor.TYPE_ACCELEROMETER)
        sensor(Sensor.TYPE_GAME_ROTATION_VECTOR)
        sensors.setForceListenersToFail(true)
        val packets = mutableListOf<ByteArray>()
        val engine = engine(packets = packets)
        engine.start()
        advance(30)
        assertFalse(sensors.hasListener(engine))
        assertEquals(3, packets.size)
        assertEquals(0, packets.last()[5].toInt() and PmpCodec.FLAG_QUAT_VALID)
        assertArrayEquals(floatArrayOf(1f, 0f, 0f, 0f), packets.last().floats(24, 4), 0f)
    }

    @Test
    @Config(shadows = [DeniedSensorManager::class])
    fun deniedSensorRegistrationDoesNotPreventInput() {
        sensor(Sensor.TYPE_GYROSCOPE)
        sensor(Sensor.TYPE_ACCELEROMETER)
        sensor(Sensor.TYPE_GAME_ROTATION_VECTOR)
        val packets = mutableListOf<ByteArray>()
        engine(packets = packets).start()
        advance(20)
        assertEquals(2, packets.size)
    }

    @Test fun tiltFlagIsOffByDefaultAndSetInEveryFormatWhenRequested() {
        for (kind in SenderKind.entries) {
            val packets = mutableListOf<ByteArray>()
            val engine = engine(kind, packets).apply { start() }
            advance(20)
            assertTrue("$kind: sin pedirlo, el bit4 va a 0 (los bytes de siempre)",
                packets.isNotEmpty() && packets.all { it[5].toInt() and PmpCodec.FLAG_TILT == 0 })
            val before = packets.size
            engine.tilt = true // Ajustes con el enlace vivo
            advance(20)
            val after = packets.drop(before)
            assertTrue("$kind: el bit4 entra al instante", after.isNotEmpty() && after.all { it[5].toInt() and PmpCodec.FLAG_TILT != 0 })
            engine.tilt = false
            val before2 = packets.size
            advance(20)
            assertTrue("$kind: y sale igual", packets.drop(before2).all { it[5].toInt() and PmpCodec.FLAG_TILT == 0 })
            stop(engine)
        }
    }

    @Test fun tiltKeepsSendingTheSensorsUnchanged() {
        val accel = sensor(Sensor.TYPE_ACCELEROMETER)
        val packets = mutableListOf<ByteArray>()
        engine(packets = packets).apply { tilt = true; start() }
        emit(accel, 1f, 2f, 9.8f)
        advance(20)
        val last = packets.last()
        assertEquals(72, last.size)
        // Sin rotation vector el bit0 sigue a 0; el bit4 pedido, y el acelerómetro tal cual
        assertEquals(PmpCodec.FLAG_TILT, last[5].toInt() and (PmpCodec.FLAG_TILT or PmpCodec.FLAG_QUAT_VALID))
        assertArrayEquals(floatArrayOf(1f, 2f, 9.8f), last.floats(52, 3), 0.0001f)
    }

    @Implements(SensorManager::class)
    class DeniedSensorManager : ShadowSensorManager() {
        @Implementation
        public override fun registerListener(listener: SensorEventListener?, sensor: Sensor?, rate: Int, handler: Handler?): Boolean {
            throw SecurityException("Sensor access denied")
        }
    }

    @Test fun stalledGyroFallsBackToZeroAndRecoversWithoutDuplicateSends() {
        val gyro = sensor(Sensor.TYPE_GYROSCOPE)
        val packets = mutableListOf<ByteArray>()
        engine(packets = packets).start()
        repeat(20) { advance(4); emit(gyro, 1f, 2f, 3f) }
        assertEquals("The active gyro keeps its 250 Hz route", 20, packets.size)
        assertArrayEquals(floatArrayOf(1f, 2f, 3f), packets.last().floats(40, 3), 0f)
        val beforeStall = packets.size
        advance(50)
        assertEquals("The fallback restores a 100 Hz input stream", 5, packets.size - beforeStall)
        assertArrayEquals(FloatArray(3), packets.last().floats(40, 3), 0f)
        val beforeRecovery = packets.size
        repeat(20) { advance(4); emit(gyro, 4f, 5f, 6f) }
        assertEquals("No extra heartbeat while gyro is healthy", 20, packets.size - beforeRecovery)
        assertArrayEquals(floatArrayOf(4f, 5f, 6f), packets.last().floats(40, 3), 0f)
        packets.zipWithNext().forEach { (a, b) -> assertTrue(b.long(16) - a.long(16) >= 4000L) }
    }

    @Test fun fiftyHzGyroKeepsMotionBetweenItsSamples() {
        val gyro = sensor(Sensor.TYPE_GYROSCOPE)
        val packets = mutableListOf<ByteArray>()
        engine(packets = packets).start()
        emit(gyro, 1f, 2f, 3f)
        repeat(10) { advance(20); emit(gyro, 1f, 2f, 3f) }
        assertTrue("Controls keep flowing between 50 Hz samples", packets.size >= 20)
        packets.forEach { assertArrayEquals(floatArrayOf(1f, 2f, 3f), it.floats(40, 3), 0f) }
        advance(30)
        assertArrayEquals("A short pause is still fresh motion", floatArrayOf(1f, 2f, 3f), packets.last().floats(40, 3), 0f)
        advance(20)
        assertArrayEquals("A genuinely stalled sensor becomes neutral", FloatArray(3), packets.last().floats(40, 3), 0f)
    }

    @Test fun slightlyDelayedGyroSamplesArePreservedForTheNextInputPacket() {
        val gyro = sensor(Sensor.TYPE_GYROSCOPE)
        val packets = mutableListOf<ByteArray>()
        engine(packets = packets).start()
        repeat(10) {
            advance(20)
            // The heartbeat may already have sent at this time: the sample
            // must still be retained even though its timestamp cannot send again.
            sensors.sendSensorEventToListeners(SensorEventBuilder.newBuilder().setSensor(gyro)
                .setTimestamp(SystemClock.elapsedRealtimeNanos() - 5_000_000L)
                .setValues(floatArrayOf(4f, 5f, 6f)).build(), gyro)
        }
        advance(10)
        packets.drop(2).forEach { assertArrayEquals(floatArrayOf(4f, 5f, 6f), it.floats(40, 3), 0f) }
    }

    @Test fun fastGyroIsCappedAt250Hz() {
        val gyro = sensor(Sensor.TYPE_GYROSCOPE)
        val packets = mutableListOf<ByteArray>()
        engine(packets = packets).start()
        repeat(100) { advance(1); emit(gyro, 1f, 0f, 0f) }
        assertTrue(packets.size in 25..26)
        packets.zipWithNext().forEach { (a, b) -> assertTrue(b.long(16) - a.long(16) >= 4000L) }
    }

    @Test fun queuedGyroBurstUsesRealSendTimeAndRetainsFreshSamples() {
        val gyro = sensor(Sensor.TYPE_GYROSCOPE)
        val packets = mutableListOf<ByteArray>()
        val sentAt = mutableListOf<Long>()
        engine(packets = packets, sentAt = sentAt).start()
        val now = SystemClock.elapsedRealtimeNanos()
        repeat(8) { index ->
            sensors.sendSensorEventToListeners(SensorEventBuilder.newBuilder().setSensor(gyro)
                .setTimestamp(now - (7 - index) * 4_000_000L)
                .setValues(floatArrayOf((index + 1).toFloat(), 0f, 0f)).build(), gyro)
        }
        assertEquals("Eight queued callbacks at one instant can send only once", 1, packets.size)
        advance(4)
        emit(gyro, 9f, 0f, 0f)
        assertEquals(2, packets.size)
        assertEquals("Fresh queued samples contribute to the next weighted average", 5.5f, packets.last().floats(40, 3)[0], 0.0001f)
        sentAt.zipWithNext().forEach { (a, b) -> assertTrue("Real sends stay below 250 Hz", b - a >= 4_000_000L) }
        packets.forEachIndexed { index, packet -> assertEquals(sentAt[index] / 1000L, packet.long(16)) }
    }

    @Test fun obsoleteGyroCallbackCannotReviveMotion() {
        val gyro = sensor(Sensor.TYPE_GYROSCOPE)
        val packets = mutableListOf<ByteArray>()
        engine(packets = packets).start()
        sensors.sendSensorEventToListeners(SensorEventBuilder.newBuilder().setSensor(gyro)
            .setTimestamp(SystemClock.elapsedRealtimeNanos() - 60_000_000L)
            .setValues(floatArrayOf(8f, 9f, 10f)).build(), gyro)
        assertTrue("An obsolete sample does not trigger an INPUT", packets.isEmpty())
        advance(10)
        assertEquals(1, packets.size)
        assertArrayEquals(FloatArray(3), packets.last().floats(40, 3), 0f)
    }

    @Test fun firstButtonAfterLongStationaryIdleUsesTheNextAvailableSendSlot() {
        val packets = mutableListOf<ByteArray>()
        val sentAt = mutableListOf<Long>()
        engine(packets = packets, sentAt = sentAt).start()
        // Thirty seconds of neutral input, including battery refreshes, without any sensor.
        repeat(3000) {
            ShadowSystemClock.advanceBy(Duration.ofMillis(10))
            loopers.values.forEach { it.idle() }
        }
        advance(1)
        val beforePress = packets.size
        ButtonState.set(ButtonState.A, true)
        loopers.values.forEach { it.idle() }
        assertEquals("Button input must respect the same 250 Hz limit", beforePress, packets.size)
        advance(3)
        assertEquals("The first press must not wait for the 10 ms fallback tick", beforePress + 1, packets.size)
        assertEquals(ButtonState.A, packets.last().int(64))
        assertEquals(4_000_000L, sentAt.last() - sentAt[sentAt.lastIndex - 1])
        assertArrayEquals(FloatArray(3), packets.last().floats(40, 3), 0f)
    }

    @Test fun batchedDownAndUpStillEmitAPressAndAReleaseBeforeFallbackTicks() {
        val packets = mutableListOf<ByteArray>()
        engine(packets = packets).start()
        advance(101)
        // Android can deliver a delayed DOWN and UP together. The latch starts
        // from delivery time, so the physical tap remains visible in INPUT.
        ButtonState.set(ButtonState.B, true)
        ButtonState.set(ButtonState.B, false)
        advance(3)
        assertEquals(ButtonState.B, packets.last().int(64))
        advance(66)
        assertEquals(ButtonState.B, packets.last().int(64))
        advance(4)
        assertEquals("The minimum hold is not extended to the next heartbeat", 0, packets.last().int(64))
        assertEquals(listOf(0, ButtonState.B, 0), packets.map { it.int(64) }.distinctUntilChanged())
    }

    @Test fun twoQuickTapsKeepBothEdgesAndTheSharedRateLimit() {
        val packets = mutableListOf<ByteArray>()
        val sentAt = mutableListOf<Long>()
        engine(packets = packets, sentAt = sentAt).start()
        advance(101)
        ButtonState.set(ButtonState.A, true)
        advance(20)
        ButtonState.set(ButtonState.A, false)
        advance(20)
        ButtonState.set(ButtonState.A, true)
        ButtonState.set(ButtonState.A, false)
        advance(200)
        assertEquals(listOf(0, ButtonState.A, 0, ButtonState.A, 0), packets.map { it.int(64) }.distinctUntilChanged())
        sentAt.zipWithNext().forEach { (a, b) -> assertTrue("Buttons share the sensor rate limit", b - a >= 4_000_000L) }
        packets.zipWithNext().forEach { (a, b) -> assertEquals(a.int(12) + 1, b.int(12)) }
    }

    @Test fun twoTapsDeliveredTogetherCannotEraseTheFirstPressBeforeTheSenderRuns() {
        val packets = mutableListOf<ByteArray>()
        engine(packets = packets).start()
        advance(101)
        ButtonState.set(ButtonState.A, true)
        ButtonState.set(ButtonState.A, false)
        ButtonState.set(ButtonState.A, true)
        ButtonState.set(ButtonState.A, false)
        advance(220)
        assertEquals("Each delivered tap needs its own press and release on the wire",
            listOf(0, ButtonState.A, 0, ButtonState.A, 0), packets.map { it.int(64) }.distinctUntilChanged())
    }

    @Test fun compressedTapsRemainVisibleAtEverySixtyHzPollingPhase() {
        val packets = mutableListOf<ByteArray>()
        val sentAt = mutableListOf<Long>()
        engine(packets = packets, sentAt = sentAt).start()
        advance(101)
        ButtonState.set(ButtonState.A, true)
        ButtonState.set(ButtonState.A, false)
        ButtonState.set(ButtonState.A, true)
        ButtonState.set(ButtonState.A, false)
        advance(220)
        val edges = packets.indices.filter { it == 0 || packets[it].int(64) != packets[it - 1].int(64) }
        assertEquals(listOf(0, ButtonState.A, 0, ButtonState.A, 0), edges.map { packets[it].int(64) })
        val times = edges.map { sentAt[it] }
        assertTrue("First tap must remain on the wire for at least 70 ms", times[2] - times[1] >= 70_000_000L)
        assertTrue("The release between taps must span a 60 Hz frame", times[3] - times[2] >= 20_000_000L)
        assertTrue("Second tap must also remain on the wire for at least 70 ms", times[4] - times[3] >= 70_000_000L)
        // Model an emulator that only keeps the latest packet and polls once
        // per video frame. Vary its phase independently of the sender clock.
        for (phase in 0L until 16_666_667L step 1_000_000L) {
            val observed = mutableListOf<Int>()
            var at = sentAt.first() + phase
            while (at <= sentAt.last()) {
                val latest = sentAt.indexOfLast { it <= at }
                observed += packets[latest].int(64)
                at += 16_666_667L
            }
            assertEquals("A 60 Hz consumer must see both taps at phase $phase ns",
                listOf(0, ButtonState.A, 0, ButtonState.A, 0), observed.distinctUntilChanged())
        }
    }

    @Test fun aPendingAReleaseCannotDelayANewBPress() {
        val packets = mutableListOf<ByteArray>()
        engine(packets = packets).start()
        advance(101)
        ButtonState.set(ButtonState.A, true)
        ButtonState.set(ButtonState.A, false)
        ButtonState.set(ButtonState.A, true)
        ButtonState.set(ButtonState.A, false)
        advance(4)
        ButtonState.set(ButtonState.B, true)
        advance(3)
        assertEquals("B starts at the next send slot while A finishes its minimum hold",
            ButtonState.A or ButtonState.B, packets.last().int(64))
    }

    @Test fun aStalledSenderMeasuresTheHoldFromTheFirstPacketItActuallySends() {
        val packets = mutableListOf<ByteArray>()
        val sentAt = mutableListOf<Long>()
        engine(packets = packets, sentAt = sentAt).start()
        advance(101)
        ButtonState.set(ButtonState.A, true)
        ButtonState.set(ButtonState.A, false)
        // The UI release happens while the network/sensor handler is delayed.
        ShadowSystemClock.advanceBy(Duration.ofMillis(100))
        shadowOf(android.os.Looper.getMainLooper()).idle()
        loopers.values.forEach { it.idle() }
        advance(80)
        val pressed = packets.indexOfFirst { it.int(64) == ButtonState.A }
        val released = packets.indices.first { it > pressed && packets[it].int(64) == 0 }
        assertTrue("A late handler must not collapse an already queued tap into a 4 ms pulse",
            sentAt[released] - sentAt[pressed] >= 70_000_000L)
    }

    @Test fun aSensorPacketCanSatisfyThePendingButtonSend() {
        val gyro = sensor(Sensor.TYPE_GYROSCOPE)
        val packets = mutableListOf<ByteArray>()
        engine(packets = packets).start()
        advance(4)
        emit(gyro, 1f, 2f, 3f)
        ButtonState.set(ButtonState.A, true)
        loopers.values.forEach { it.idle() }
        advance(3)
        ShadowSystemClock.advanceBy(Duration.ofMillis(1))
        emit(gyro, 4f, 5f, 6f)
        loopers.values.forEach { it.idle() }
        assertEquals(2, packets.size)
        assertEquals(ButtonState.A, packets.last().int(64))
        advance(4)
        assertEquals("An already delivered button change must not cause a duplicate INPUT", 2, packets.size)
    }

    @Test fun resetReleasesButtonsPromptlyAndStoppedEnginesCannotObserveNewInput() {
        val oldPackets = mutableListOf<ByteArray>()
        val old = engine(packets = oldPackets)
        old.start()
        advance(101)
        ButtonState.set(ButtonState.A, true)
        advance(3)
        assertEquals(ButtonState.A, oldPackets.last().int(64))
        ButtonState.reset()
        advance(4)
        assertEquals(0, oldPackets.last().int(64))
        stop(old)
        val stoppedCount = oldPackets.size
        val newPackets = mutableListOf<ByteArray>()
        engine(packets = newPackets).start()
        ButtonState.set(ButtonState.B, true)
        loopers.values.forEach { it.idle() }
        assertEquals("A replacement engine receives input without an extra heartbeat", ButtonState.B, newPackets.single().int(64))
        advance(20)
        assertEquals(stoppedCount, oldPackets.size)
    }

    @Test fun leavingTheControllerCancelsUnsentButtonEdges() {
        val packets = mutableListOf<ByteArray>()
        engine(packets = packets).start()
        advance(101)
        ButtonState.set(ButtonState.A, true)
        ButtonState.set(ButtonState.A, false)
        ButtonState.set(ButtonState.A, true)
        ButtonState.set(ButtonState.A, false)
        // A controller screen or mode closes before the sender gets its turn.
        ButtonState.reset()
        advance(100)
        assertTrue("No queued button press may leak into the next screen or mode",
            packets.all { it.int(64) == 0 })
    }

    private fun <T> List<T>.distinctUntilChanged(): List<T> =
        filterIndexed { index, item -> index == 0 || item != this[index - 1] }

    @Test fun stopCancelsAllSendsAndReplacementEngineHasOneLoop() {
        val gyro = sensor(Sensor.TYPE_GYROSCOPE)
        val oldPackets = mutableListOf<ByteArray>()
        val old = engine(packets = oldPackets)
        old.start()
        old.start()
        advance(20)
        assertEquals("Starting twice must not duplicate the loop", 2, oldPackets.size)
        stop(old)
        stop(old)
        val stoppedCount = oldPackets.size
        old.onSensorChanged(SensorEventBuilder.newBuilder().setSensor(gyro)
            .setTimestamp(SystemClock.elapsedRealtimeNanos() + 4_000_000L).setValues(floatArrayOf(1f, 2f, 3f)).build())
        advance(30)
        assertEquals("Even a queued event after stop cannot send", stoppedCount, oldPackets.size)
        assertFalse(sensors.hasListener(old))
        val newPackets = mutableListOf<ByteArray>()
        engine(packets = newPackets).start()
        advance(30)
        assertEquals(stoppedCount, oldPackets.size)
        assertEquals(3, newPackets.size)
    }
}
