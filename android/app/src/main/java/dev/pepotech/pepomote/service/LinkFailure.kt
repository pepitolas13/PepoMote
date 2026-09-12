package dev.pepotech.pepomote.service

/**
 * Qué hacer cuando el enlace muere con un `err` del receptor (o "io" de la
 * red): función pura, sin Android, testeada en LinkFailureTest.
 */
object LinkFailure {
    /** Códigos con los que el PC rechaza el emparejamiento guardado. */
    private val NEEDS_QR = setOf("bad_token", "bad_code")

    /**
     * El PC ya no acepta el token guardado (PepoMote reinstalado o su
     * `token.txt` regenerado): solo un QR nuevo lo arregla, así que en vez de
     * un aviso y vuelta al inicio se abre la pantalla Conectar con el escáner.
     */
    fun needsNewQr(code: String): Boolean = code in NEEDS_QR

    /**
     * Fallo de red con varios PCs guardados: en vez de un aviso y al inicio,
     * la pantalla Conectar con la explicación (quizá otro PC sí está). null =
     * un solo PC, o un error que no es de red: aviso y al inicio, como siempre.
     */
    fun afterIo(code: String, pcName: String?, savedCount: Int): String? {
        if (code != "io" || savedCount < 2) return null
        val pc = pcName?.trim()?.takeIf { it.isNotEmpty() } ?: "Tu PC"
        return "$pc no responde. Elige otro PC o escanea un QR."
    }

    /** Explicación para la pantalla Conectar cuando toca volver a escanear. */
    fun rePairReason(pcName: String?): String {
        val pc = pcName?.trim()?.takeIf { it.isNotEmpty() } ?: "Tu PC"
        return "$pc ya no reconoce este móvil: su QR ha cambiado (PepoMote reinstalado o " +
            "restablecido en el PC). Escanéalo otra vez y sigues donde estabas."
    }
}
