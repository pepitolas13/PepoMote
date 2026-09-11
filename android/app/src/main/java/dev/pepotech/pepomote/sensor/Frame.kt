package dev.pepotech.pepomote.sensor

/**
 * Remapeo de los sensores para el GamePad (móvil apaisado). El receptor y
 * Cemu esperan el marco de un DS4 tumbado: X = derecha del mando, Y = hacia
 * el borde superior del mando (lejos del jugador), Z = saliendo de la
 * pantalla. En vertical (Wiimote) coincide con los ejes del móvil; apaisado
 * hay que girar los vectores ANTES de escribir el paquete, según hacia dónde
 * quede el borde superior del móvil. Puro, sin Android: testeable.
 *
 * Las rotaciones son las de `android.view.Surface` (mismos valores).
 */
object Frame {
    const val ROTATION_0 = 0
    const val ROTATION_90 = 1
    const val ROTATION_180 = 2
    const val ROTATION_270 = 3

    private const val HALF_SQRT2 = 0.70710678f

    /**
     * Vector (accel o gyro): borde superior a la izquierda (ROTATION_90) →
     * (−y, x, z); a la derecha (ROTATION_270) → (y, −x, z); cualquier otra
     * rotación, tal cual.
     */
    private fun remapVector(src: FloatArray, rotation: Int, out: FloatArray): FloatArray {
        val x = src[0]
        val y = src[1]
        val z = src[2]
        when (rotation) {
            ROTATION_90 -> {
                out[0] = -y; out[1] = x; out[2] = z
            }

            ROTATION_270 -> {
                out[0] = y; out[1] = -x; out[2] = z
            }

            else -> {
                out[0] = x; out[1] = y; out[2] = z
            }
        }
        return out
    }

    /** Acelerómetro (m/s²) al marco del GamePad. `out` puede ser `accel` (en sitio). */
    fun remapAccel(accel: FloatArray, rotation: Int, out: FloatArray = FloatArray(3)): FloatArray =
        remapVector(accel, rotation, out)

    /** Giroscopio (rad/s) al marco del GamePad. `out` puede ser `gyro` (en sitio). */
    fun remapGyro(gyro: FloatArray, rotation: Int, out: FloatArray = FloatArray(3)): FloatArray =
        remapVector(gyro, rotation, out)

    /**
     * Quaternion (w, x, y, z) al marco del GamePad: `quat ⊗ r` (producto de
     * Hamilton, r a la derecha) con r = (√½, 0, 0, −√½) si el borde superior
     * queda a la izquierda y r = (√½, 0, 0, +√½) si queda a la derecha.
     * Cualquier otra rotación: tal cual. `out` puede ser `quat` (en sitio).
     */
    fun remapQuat(quat: FloatArray, rotation: Int, out: FloatArray = FloatArray(4)): FloatArray {
        val rz = when (rotation) {
            ROTATION_90 -> -HALF_SQRT2
            ROTATION_270 -> HALF_SQRT2
            else -> {
                out[0] = quat[0]; out[1] = quat[1]; out[2] = quat[2]; out[3] = quat[3]
                return out
            }
        }
        val rw = HALF_SQRT2
        val qw = quat[0]
        val qx = quat[1]
        val qy = quat[2]
        val qz = quat[3]
        // q ⊗ r con r = (rw, 0, 0, rz)
        out[0] = qw * rw - qz * rz
        out[1] = qx * rw + qy * rz
        out[2] = qy * rw - qx * rz
        out[3] = qw * rz + qz * rw
        return out
    }
}
