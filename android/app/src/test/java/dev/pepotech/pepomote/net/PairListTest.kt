package dev.pepotech.pepomote.net

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class PairListTest {

    private val salon = Pairing("192.168.1.5", 26761, "tok-salon", "SALÓN-PC")
    private val cuarto = Pairing("192.168.1.9", 26800, "tok-cuarto", "Cuarto & Co = raro")

    @Test
    fun idaYVueltaConCaracteresRaros() {
        val list = listOf(salon, cuarto, Pairing("10.0.0.2", 26761, "a=b&c", "ñandú"))
        assertEquals(list, PairList.decode(PairList.encode(list)))
        assertEquals(3, PairList.encode(list).lines().size, "una línea por PC")
        assertTrue(PairList.decode("").isEmpty())
        assertTrue(PairList.decode(null).isEmpty())
        assertTrue(PairList.decode("basura sin campos").isEmpty(), "una línea rota se ignora")
        assertEquals(listOf(salon), PairList.decode(PairList.encode(listOf(salon)) + "\n\nhost=1.2.3.4&port=1"), "sin token no es un PC")
    }

    @Test
    fun upsertPorTokenYForget() {
        val one = PairList.upsert(emptyList(), salon)
        val two = PairList.upsert(one, cuarto)
        assertEquals(listOf(salon, cuarto), two)
        val renamed = PairList.upsert(two, salon.copy(pcName = "SALON", host = "192.168.1.50"))
        assertEquals(2, renamed.size, "mismo token: se actualiza, no se duplica")
        assertEquals("SALON", renamed[0].pcName)
        assertEquals("192.168.1.50", renamed[0].host)
        assertEquals(listOf(cuarto), PairList.forget(two, "tok-salon"))
        assertEquals(two, PairList.forget(two, "no-existe"))
    }

    @Test
    fun elActualTrasOlvidar() {
        val two = listOf(salon, cuarto)
        assertEquals("tok-cuarto", PairList.nextCurrent(PairList.forget(two, "tok-salon"), "tok-salon", "tok-salon"), "se olvida el actual: el primero que queda")
        assertEquals("tok-salon", PairList.nextCurrent(PairList.forget(two, "tok-cuarto"), "tok-salon", "tok-cuarto"), "se olvida otro: el actual sigue")
        assertNull(PairList.nextCurrent(emptyList(), "tok-salon", "tok-salon"))
        assertEquals("tok-salon", PairList.nextCurrent(two, null, "x"), "sin actual: el primero")
    }

    @Test
    fun enLaRedYDesconocidos() {
        val seen = listOf(
            ReceiverInfo("SALÓN-PC", "192.168.1.5", 26761),
            ReceiverInfo("Otro", "192.168.1.9", 26800),
            ReceiverInfo("Nuevo", "192.168.1.77", 26761)
        )
        assertTrue(PairList.isOnline(salon, seen), "por nombre")
        assertTrue(PairList.isOnline(cuarto, seen), "por IP aunque se llame distinto")
        assertFalse(PairList.isOnline(Pairing("1.1.1.1", 1, "t", "Nadie"), seen))
        assertEquals(listOf(seen[2]), PairList.unknown(seen, listOf(salon, cuarto)))
    }

    @Test
    fun recolocarPorNombreSinPisarOtroPc() {
        val moved = listOf(ReceiverInfo("SALÓN-PC", "192.168.1.60", 26761))
        assertEquals("192.168.1.60", PairList.relocateTarget(salon, listOf(salon, cuarto), moved).host)
        assertEquals(salon, PairList.relocateTarget(salon, listOf(salon), listOf(ReceiverInfo("SALÓN-PC", "192.168.1.5", 26761))), "mismo sitio: nada")
        // el nombre de SALÓN aparece justo donde vive Cuarto (otro token): no se adopta
        val clash = listOf(ReceiverInfo("SALÓN-PC", "192.168.1.9", 26800))
        assertEquals(salon, PairList.relocateTarget(salon, listOf(salon, cuarto), clash))
        val refreshed = PairList.refreshFrom(listOf(salon, cuarto), moved)
        assertEquals("192.168.1.60", refreshed[0].host)
        assertEquals(cuarto, refreshed[1])
    }

    private fun assertEquals(expected: Int, actual: Int, message: String) =
        org.junit.Assert.assertEquals(message, expected.toLong(), actual.toLong())

    private fun assertEquals(expected: Any?, actual: Any?, message: String) =
        org.junit.Assert.assertEquals(message, expected, actual)

    private fun assertTrue(value: Boolean, message: String) = org.junit.Assert.assertTrue(message, value)
}
