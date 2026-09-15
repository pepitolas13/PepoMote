package dev.pepotech.pepomote.sensor

import android.content.pm.PackageManager
import org.junit.Assert.assertFalse
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class SensorCompatibilityTest {
    @Test fun installingTheControllerDoesNotRequireMotionSensors() {
        val app = RuntimeEnvironment.getApplication()
        val info = app.packageManager.getPackageInfo(app.packageName, PackageManager.GET_CONFIGURATIONS)
        val required = info.reqFeatures.orEmpty().filter { it.flags and 1 != 0 }.mapNotNull { it.name }
        assertFalse("Motion sensors are optional: $required", required.any { it.startsWith("android.hardware.sensor.") })
    }
}
