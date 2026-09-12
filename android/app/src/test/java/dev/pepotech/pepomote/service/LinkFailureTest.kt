package dev.pepotech.pepomote.service

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
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
    fun laExplicacionNombraAlPc() {
        assertTrue(LinkFailure.rePairReason("SALON-PC").startsWith("SALON-PC ya no reconoce este móvil"))
        assertTrue(LinkFailure.rePairReason(" SALON-PC ").startsWith("SALON-PC ya no reconoce"))
        assertTrue(LinkFailure.rePairReason(null).startsWith("Tu PC ya no reconoce"))
        assertTrue(LinkFailure.rePairReason("   ").startsWith("Tu PC ya no reconoce"))
        assertTrue(LinkFailure.rePairReason("PC").contains("Escanéalo otra vez"))
    }

    @Test
    fun conVariosPcsUnFalloDeRedOfreceElegirOtro() {
        assertEquals("SALON-PC no responde. Elige otro PC o escanea un QR.", LinkFailure.afterIo("io", "SALON-PC", 2))
        assertEquals("Tu PC no responde. Elige otro PC o escanea un QR.", LinkFailure.afterIo("io", " ", 3))
        assertNull("con un solo PC no hay nada que elegir", LinkFailure.afterIo("io", "SALON-PC", 1))
        assertNull("un error del PC no es de red", LinkFailure.afterIo("busy", "SALON-PC", 2))
    }
}
