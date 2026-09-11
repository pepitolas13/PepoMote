package dev.pepotech.pepomote.service

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
    fun laExplicacionNombraAlPc() {
        assertTrue(LinkFailure.rePairReason("SALON-PC").startsWith("SALON-PC ya no reconoce este móvil"))
        assertTrue(LinkFailure.rePairReason(" SALON-PC ").startsWith("SALON-PC ya no reconoce"))
        assertTrue(LinkFailure.rePairReason(null).startsWith("Tu PC ya no reconoce"))
        assertTrue(LinkFailure.rePairReason("   ").startsWith("Tu PC ya no reconoce"))
        assertTrue(LinkFailure.rePairReason("PC").contains("Escanéalo otra vez"))
    }
}
