package dev.pepotech.pepomote.control

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** «Teclado» del modo Wii U: codificación del mensaje `text` y semántica de sus botones. */
class TextInputTest {

    @Test
    fun mensajeText() {
        assertEquals("{\"m\":\"text\",\"text\":\"Link\"}", TextInput.encode("Link"))
        assertEquals("{\"m\":\"text\",\"text\":\"\"}", TextInput.encode(""))
    }

    @Test
    fun introYBorrarVanEscapadosComoJson() {
        assertEquals("\n", TextInput.ENTER)
        assertEquals(1, TextInput.BACKSPACE.length)
        assertEquals(0x08, TextInput.BACKSPACE[0].code)
        assertEquals("{\"m\":\"text\",\"text\":\"Link\\n\"}", TextInput.encode("Link\n"))
        assertEquals("{\"m\":\"text\",\"text\":\"\\b\"}", TextInput.encode("\u0008"))
        assertEquals("{\"m\":\"text\",\"text\":\"\\n\"}", TextInput.encode("\n"))
    }

    @Test
    fun escapadoJsonDeComillasBarrasControlesYUnicode() {
        assertEquals(
            "{\"m\":\"text\",\"text\":\"a\\\"b\\\\c\\/d\\t\\r\\f\\u0001 ñ 日本\"}",
            TextInput.encode("a\"b\\c/d\t\r\u000c\u0001 ñ 日本")
        )
    }

    @Test
    fun borrarMandaUnBorradoYNoTocaElCampo() {
        val a = TextInput.delete("Li")
        assertEquals(TextInput.BACKSPACE, a.send)
        assertEquals("Li", a.field)
        assertFalse(a.close)
    }

    @Test
    fun escribirMandaElCampoTalCualYLoVacia() {
        val a = TextInput.write("Link")
        assertEquals("Link", a.send)
        assertEquals("", a.field)
        assertFalse(a.close)
        // Vacío: nada que mandar, el diálogo sigue
        val v = TextInput.write("")
        assertNull(v.send)
        assertEquals("", v.field)
        assertFalse(v.close)
    }

    @Test
    fun aceptarMandaElCampoMasIntroYCierra() {
        val a = TextInput.accept("Link")
        assertEquals("Link\n", a.send)
        assertEquals("", a.field)
        assertTrue(a.close)
        // Vacío: solo Intro (sirve para confirmar)
        val v = TextInput.accept("")
        assertEquals("\n", v.send)
        assertTrue(v.close)
    }

    @Test
    fun cerrarNoMandaNada() {
        val a = TextInput.close("Li")
        assertNull(a.send)
        assertEquals("Li", a.field)
        assertTrue(a.close)
    }
}
