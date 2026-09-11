package dev.pepotech.pepomote.net

import android.graphics.Bitmap
import android.graphics.BitmapFactory

/**
 * JPEG → [Bitmap] con `BitmapFactory` reutilizando los bitmaps (`inBitmap`,
 * `inMutable`): tres en rotación, para que el que se está decodificando
 * nunca sea el que la UI está pintando ni el que el renderizador acaba de
 * subir a textura. Nada se reserva por imagen; si el receptor cambia el
 * tamaño, se reasigna una vez y se vuelve a reutilizar.
 */
class BitmapDecoder(slots: Int = SLOTS) : ScreenDecoder<Bitmap> {

    private companion object {
        const val SLOTS = 3
    }

    private val pool = arrayOfNulls<Bitmap>(slots)
    private var next = 0
    private val options = BitmapFactory.Options().apply {
        inMutable = true
        inScaled = false
        inPreferredConfig = Bitmap.Config.ARGB_8888
    }

    override fun decode(data: ByteArray, off: Int, len: Int): Bitmap? {
        val slot = next
        options.inBitmap = pool[slot]
        val bitmap = try {
            BitmapFactory.decodeByteArray(data, off, len, options)
        } catch (_: IllegalArgumentException) {
            // El bitmap reutilizado no vale para esta imagen (otro tamaño): uno nuevo
            options.inBitmap = null
            try {
                BitmapFactory.decodeByteArray(data, off, len, options)
            } catch (_: Exception) {
                null
            }
        } ?: return null
        pool[slot] = bitmap
        next = (slot + 1) % pool.size
        return bitmap
    }
}
