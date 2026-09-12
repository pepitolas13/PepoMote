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
     * la pantalla Conectar con la explicación (quizá otro PC sí está). false =
     * un solo PC, o un error que no es de red: aviso y al inicio, como siempre.
     */
    fun offerAnotherPc(code: String, savedCount: Int): Boolean = code == "io" && savedCount >= 2

    /** Nombre del PC para las explicaciones, o `fallback` («Tu PC») si no se sabe. */
    fun pcLabel(pcName: String?, fallback: String): String =
        pcName?.trim()?.takeIf { it.isNotEmpty() } ?: fallback
}
