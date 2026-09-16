package dev.pepotech.pepomote.sensor

import android.content.Context
import android.content.pm.PackageManager
import android.hardware.Sensor
import android.hardware.SensorManager

/** Qué giroscopio tiene el móvil, de cara al puntero. */
enum class GyroClass {
    /** Giroscopio de verdad: el puntero apunta como un mando de Wii. */
    REAL,

    /**
     * Hay un giroscopio o rotation vector, pero no es real: software derivado
     * del acelerómetro (Unisoc y otros). No ve el giro sobre la vertical (el
     * cursor solo sube y baja) y puede tener deriva, retardo o inconsistencias.
     */
    VIRTUAL,

    /** Ni giroscopio ni rotation vector: solo el acelerómetro. */
    NONE
}

/**
 * Clasifica el giroscopio del móvil. Conservador a propósito: un falso
 * VIRTUAL cambiaría el puntero a un móvil que hoy funciona; un falso REAL solo
 * pierde el automatismo (el usuario elige Acelerómetro en Ajustes, donde se
 * enseña el nombre del sensor para poder diagnosticar).
 */
object GyroDetect {
    /** Palabras que delatan un giroscopio de software en su nombre o fabricante. */
    private val VIRTUAL_WORDS = listOf("virtual", "software", "pseudo", "emul", "simul", "fake", "synth", "deriv", "fusion")

    /**
     * Pura, testeable: [featureDeclared] = `FEATURE_SENSOR_GYROSCOPE` declarada
     * por el fabricante; [hasGyro] / [gyroName] / [gyroVendor] del
     * `TYPE_GYROSCOPE` por defecto; [hasRotationVector] = hay
     * `TYPE_GAME_ROTATION_VECTOR`.
     */
    fun classify(
        featureDeclared: Boolean,
        hasGyro: Boolean,
        gyroName: String?,
        gyroVendor: String?,
        hasRotationVector: Boolean
    ): GyroClass = when {
        !hasGyro && !hasRotationVector -> GyroClass.NONE
        // Un rotation vector sin giroscopio solo puede ser software
        !hasGyro -> GyroClass.VIRTUAL
        // Un giroscopio real se declara siempre (lo exige la CDD y lo usa Play)
        !featureDeclared -> GyroClass.VIRTUAL
        looksVirtual(gyroName) || looksVirtual(gyroVendor) -> GyroClass.VIRTUAL
        else -> GyroClass.REAL
    }

    fun looksVirtual(text: String?): Boolean {
        val t = text?.lowercase() ?: return false
        return VIRTUAL_WORDS.any { it in t }
    }

    @Volatile
    private var cached: GyroClass? = null

    @Volatile
    private var cachedName: String? = null

    /** Clase del giroscopio de este móvil (se calcula una vez). */
    fun classify(context: Context): GyroClass {
        cached?.let { return it }
        val app = context.applicationContext
        val sensors = app.getSystemService(Context.SENSOR_SERVICE) as? SensorManager
        val gyro = defaultSensor(sensors, Sensor.TYPE_GYROSCOPE)
        val rotation = defaultSensor(sensors, Sensor.TYPE_GAME_ROTATION_VECTOR)
        val feature = try {
            app.packageManager.hasSystemFeature(PackageManager.FEATURE_SENSOR_GYROSCOPE)
        } catch (_: RuntimeException) {
            false
        }
        val result = classify(feature, gyro != null, gyro?.name, gyro?.vendor, rotation != null)
        cachedName = describe(gyro ?: rotation)
        cached = result
        return result
    }

    /**
     * «nombre · fabricante» del giroscopio (o del rotation vector si no hay
     * giroscopio), para Ajustes; null si no hay ninguno.
     */
    fun describe(context: Context): String? {
        classify(context)
        return cachedName
    }

    private fun defaultSensor(sensors: SensorManager?, type: Int): Sensor? = try {
        sensors?.getDefaultSensor(type)
    } catch (_: RuntimeException) {
        null
    }

    private fun describe(sensor: Sensor?): String? {
        val s = sensor ?: return null
        val parts = listOf(s.name, s.vendor).filter { !it.isNullOrBlank() }
        return parts.joinToString(" · ").ifBlank { null }
    }
}
