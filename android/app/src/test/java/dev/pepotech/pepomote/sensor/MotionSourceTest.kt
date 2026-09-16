package dev.pepotech.pepomote.sensor

import android.content.Context
import android.content.pm.PackageManager
import android.hardware.Sensor
import android.hardware.SensorManager
import dev.pepotech.pepomote.control.AppPrefs
import dev.pepotech.pepomote.service.LinkState
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.shadows.ShadowSensor

/**
 * El selector de Ajustes cambia el sensor en el momento, haya enlace o no:
 * `choose` publica siempre el [MotionSource.tilt] que leen los chips. Sin
 * enlace vivo (`LinkState.setTilt` null) el `refresh` iba dentro del
 * argumento de una llamada segura y no se evaluaba: la preferencia se
 * guardaba pero los chips no se movían hasta volver a entrar.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class MotionSourceTest {
    @Before fun setUp() {
        val app = RuntimeEnvironment.getApplication()
        // Móvil con giroscopio de verdad: si no, solo cabe el acelerómetro
        // (GyroClass.NONE) y elegir Giroscopio no cambiaría nada
        shadowOf(app.packageManager).setSystemFeature(PackageManager.FEATURE_SENSOR_GYROSCOPE, true)
        val sensors = app.getSystemService(Context.SENSOR_SERVICE) as SensorManager
        shadowOf(sensors).addSensor(ShadowSensor.newInstance(Sensor.TYPE_GYROSCOPE))
        // La clase del giroscopio se calcula una vez por proceso: se olvida lo
        // que hubiera cacheado otra prueba del mismo sandbox
        for (field in listOf("cached", "cachedName")) {
            GyroDetect::class.java.getDeclaredField(field).apply { isAccessible = true }.set(GyroDetect, null)
        }
        LinkState.setTilt = null
    }

    @Test fun elSelectorPublicaElSensorSinEnlaceVivo() {
        val context: Context = RuntimeEnvironment.getApplication()
        MotionSource.choose(context, accel = true)
        assertEquals(true, MotionSource.tilt.value)
        assertEquals(AppPrefs.MOTION_ACCEL, AppPrefs.motionSource(context))

        MotionSource.choose(context, accel = false)
        assertEquals(false, MotionSource.tilt.value)
        assertEquals(AppPrefs.MOTION_GYRO, AppPrefs.motionSource(context))
    }
}
