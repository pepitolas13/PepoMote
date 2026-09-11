package dev.pepotech.pepomote.control

/**
 * «Teclado» del modo Wii U: el teclado en pantalla de Cemu (nombre del
 * jugador en Zelda Wind Waker HD, etc.) no acepta toques, solo teclas del
 * PC, así que el móvil manda texto por el canal de control
 * (`{"m":"text","text":"…"}`, PROTOCOL.md §3) y el receptor lo teclea.
 * Aquí va la lógica pura del diálogo (qué se manda, qué queda en el campo y
 * si se cierra) y la codificación del mensaje, sin Android: testeado en
 * TextInputTest.
 */
object TextInput {
    /** Intro: acepta / OK en el teclado de Cemu. */
    const val ENTER = "\n"

    /** Borra un carácter en Cemu (U+0008). */
    const val BACKSPACE = "\b"

    /** Resultado de un botón del diálogo. */
    data class Action(
        /** Lo que se manda al PC (null = nada). */
        val send: String?,
        /** Contenido del campo después. */
        val field: String,
        /** Si el diálogo se cierra. */
        val close: Boolean
    )

    /** «Borrar»: un carácter menos en Cemu; el campo no cambia y el diálogo sigue. */
    fun delete(field: String) = Action(BACKSPACE, field, close = false)

    /** «Escribir»: manda el campo tal cual (nada si está vacío) y lo vacía; el diálogo sigue. */
    fun write(field: String) = Action(field.ifEmpty { null }, "", close = false)

    /** «Aceptar»: manda el campo + Intro (solo Intro con el campo vacío: confirma) y cierra. */
    fun accept(field: String) = Action(field + ENTER, "", close = true)

    /** «Cerrar» / atrás: no se manda nada. */
    fun close(field: String) = Action(null, field, close = true)

    /** Línea `{"m":"text","text":"…"}` con el texto escapado como JSON (Intro → `\n`, borrar → `\b`). */
    fun encode(text: String): String = "{\"m\":\"text\",\"text\":\"" + escape(text) + "\"}"

    /** Escapado JSON de una cadena, el mismo que hace org.json en Android. */
    fun escape(text: String): String {
        val sb = StringBuilder(text.length + 8)
        for (c in text) {
            when {
                c == '"' || c == '\\' || c == '/' -> sb.append('\\').append(c)
                c == '\t' -> sb.append("\\t")
                c == '\b' -> sb.append("\\b")
                c == '\n' -> sb.append("\\n")
                c == '\r' -> sb.append("\\r")
                c.code == 0x0c -> sb.append("\\f") // salto de página
                c.code <= 0x1f -> sb.append(String.format("\\u%04x", c.code)) // resto de controles
                else -> sb.append(c)
            }
        }
        return sb.toString()
    }
}
