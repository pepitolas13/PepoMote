package dev.pepotech.pepomote.sensor

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Clasificación pura del giroscopio: conservadora con los reales (un falso
 * VIRTUAL cambiaría el puntero a quien ya le funciona) y VIRTUAL solo con
 * pruebas: sin feature declarada, nombre de software, o rotation vector sin
 * giroscopio.
 */
class GyroDetectTest {
    @Test fun realGyroWithDeclaredFeatureIsReal() {
        assertEquals(GyroClass.REAL, GyroDetect.classify(true, true, "LSM6DSO Gyroscope", "STMicroelectronics", true))
        assertEquals(GyroClass.REAL, GyroDetect.classify(true, true, "Gyroscope Non-wakeup", "qualcomm", true))
        assertEquals(GyroClass.REAL, GyroDetect.classify(true, true, "ICM-42688 Gyroscope", "InvenSense", true))
    }

    @Test fun realGyroWithoutRotationVectorIsStillReal() {
        assertEquals(GyroClass.REAL, GyroDetect.classify(true, true, "BMI160 gyro", "Bosch", false))
    }

    @Test fun gyroWithoutDeclaredFeatureIsVirtual() {
        assertEquals(GyroClass.VIRTUAL, GyroDetect.classify(false, true, "Gyroscope", "Unisoc", true))
    }

    @Test fun softwareNamesOrVendorsAreVirtual() {
        for (name in listOf("Virtual Gyroscope", "gyro (software)", "Pseudo-Gyro", "Emulated gyroscope", "Simulated Gyro", "Gyroscope derived", "Synthetic gyro", "fake gyro")) {
            assertEquals(name, GyroClass.VIRTUAL, GyroDetect.classify(true, true, name, "x", true))
        }
        assertEquals(GyroClass.VIRTUAL, GyroDetect.classify(true, true, "Gyroscope", "Sensor Fusion Co", true))
    }

    @Test fun rotationVectorWithoutGyroIsVirtual() {
        assertEquals(GyroClass.VIRTUAL, GyroDetect.classify(true, false, null, null, true))
        assertEquals(GyroClass.VIRTUAL, GyroDetect.classify(false, false, null, null, true))
    }

    @Test fun noSensorsIsNoneWhateverTheFeatureSays() {
        assertEquals(GyroClass.NONE, GyroDetect.classify(false, false, null, null, false))
        assertEquals(GyroClass.NONE, GyroDetect.classify(true, false, null, null, false))
    }

    @Test fun looksVirtualIgnoresCaseAndNull() {
        assertTrue(GyroDetect.looksVirtual("VIRTUAL gyro"))
        assertFalse(GyroDetect.looksVirtual(null))
        assertFalse(GyroDetect.looksVirtual(""))
        assertFalse(GyroDetect.looksVirtual("ICM-42688 Gyroscope"))
    }
}
