package dev.pepotech.pepomote.service

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** Un `bad_token` lleva al escáner; cualquier otro error, aviso y al inicio. */
class LinkFailureTest {

    @Test
    fun soloElTokenYElCodigoPidenQrNuevo() {
        assertTrue(LinkFailure.needsNewQr("bad_token"))
        assertTrue(LinkFailure.needsNewQr("bad_code"))
        assertFalse(LinkFailure.needsNewQr("bad_version"))
        assertFalse(LinkFailure.needsNewQr("busy"))
        assertFalse(LinkFailure.needsNewQr("io"))
        assertFalse(LinkFailure.needsNewQr(""))
    }

    @Test
    fun elNombreDelPcOElGenerico() {
        assertEquals("SALON-PC", LinkFailure.pcLabel("SALON-PC", "Tu PC"))
        assertEquals("SALON-PC", LinkFailure.pcLabel(" SALON-PC ", "Tu PC"))
        assertEquals("Tu PC", LinkFailure.pcLabel(null, "Tu PC"))
        assertEquals("Your PC", LinkFailure.pcLabel("   ", "Your PC"))
    }

    @Test
    fun conVariosPcsUnFalloDeRedOfreceElegirOtro() {
        assertTrue(LinkFailure.offerAnotherPc("io", 2))
        assertTrue(LinkFailure.offerAnotherPc("io", 3))
        assertFalse("con un solo PC no hay nada que elegir", LinkFailure.offerAnotherPc("io", 1))
        assertFalse("un error del PC no es de red", LinkFailure.offerAnotherPc("busy", 2))
    }
}
