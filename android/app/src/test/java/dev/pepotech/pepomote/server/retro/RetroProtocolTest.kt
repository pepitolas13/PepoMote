package dev.pepotech.pepomote.server.retro

import dev.pepotech.pepomote.server.core.ReceiverInput
import org.junit.Assert.*
import org.junit.Test

/** Espejo de los tests de `desktop/src/retroarch/{protocol,mapping}.rs`. */
class RetroProtocolTest {
    @Test fun elDatagramaEsRemoteMessageEnLittleEndian() {
        // port 2, RETRO_DEVICE_JOYPAD, index 0, id A (8), state 1
        val m = RetroProtocol.encode(2, Control.button(RetroPad.A), 1)
        assertEquals(RetroProtocol.MESSAGE_LEN, m.size)
        assertArrayEquals(byteArrayOf(2, 0, 0, 0), m.copyOfRange(0, 4))
        assertArrayEquals(byteArrayOf(1, 0, 0, 0), m.copyOfRange(4, 8))
        assertArrayEquals(byteArrayOf(0, 0, 0, 0), m.copyOfRange(8, 12))
        assertArrayEquals(byteArrayOf(8, 0, 0, 0), m.copyOfRange(12, 16))
        assertArrayEquals(byteArrayOf(1, 0), m.copyOfRange(16, 18))
        assertArrayEquals(byteArrayOf(0, 0), m.copyOfRange(18, 20))
        // stick derecho, eje Y, −32767: device 5, index 1, id 1
        val a = RetroProtocol.encode(0, Control.axis(3), -32767)
        assertArrayEquals(byteArrayOf(5, 0, 0, 0), a.copyOfRange(4, 8))
        assertArrayEquals(byteArrayOf(1, 0, 0, 0), a.copyOfRange(8, 12))
        assertArrayEquals(byteArrayOf(1, 0, 0, 0), a.copyOfRange(12, 16))
        assertArrayEquals(byteArrayOf(1, -128), a.copyOfRange(16, 18))
    }

    @Test fun losIdsDelRetropadSonLosDeLibretro() {
        assertEquals(0, RetroPad.B); assertEquals(1, RetroPad.Y); assertEquals(2, RetroPad.SELECT); assertEquals(3, RetroPad.START)
        assertEquals(4, RetroPad.UP); assertEquals(7, RetroPad.RIGHT); assertEquals(8, RetroPad.A); assertEquals(9, RetroPad.X)
        assertEquals(10, RetroPad.L); assertEquals(15, RetroPad.R3)
        for (i in 0 until RetroProtocol.CONTROLS) assertEquals(i, Control(i).index)
        assertEquals(Control.axis(0), Control(16))
        assertEquals(Control.axis(3), Control(19))
        assertTrue(Control(15).isButton); assertFalse(Control(16).isButton)
    }

    @Test fun elStickDelMovilSeEscalaEInvierte() {
        assertEquals(0, RetroProtocol.axisFromStick(0, false))
        assertEquals(32767, RetroProtocol.axisFromStick(127, false))
        assertEquals(-32767, RetroProtocol.axisFromStick(-127, false))
        assertEquals(-32767, RetroProtocol.axisFromStick(-128, false))
        assertEquals(-32767, RetroProtocol.axisFromStick(127, true))
        assertTrue(RetroProtocol.axisFromStick(64, false) in 16001..16599)
    }

    @Test fun lasRespuestasSeEntienden() {
        assertEquals(Reply.Version("1.22.2"), RetroProtocol.parseReply("1.22.2\n"))
        assertEquals(Reply.Status(Activity.Contentless), RetroProtocol.parseReply("GET_STATUS CONTENTLESS"))
        assertEquals(Reply.Status(Activity.Playing("snes9x", "Super Mario World.sfc")), RetroProtocol.parseReply("GET_STATUS PLAYING snes9x,Super Mario World.sfc\n"))
        assertEquals(Reply.Status(Activity.Paused("nes", "Zelda, The (U).nes")), RetroProtocol.parseReply("GET_STATUS PAUSED nes,Zelda, The (U).nes,crc32=abcd1234\n"))
        assertEquals(Reply.Status(Activity.Contentless), RetroProtocol.parseReply("GET_STATUS ERROR"))
        assertEquals(Reply.Other("READ_CORE_MEMORY 0 -1"), RetroProtocol.parseReply("READ_CORE_MEMORY 0 -1"))
    }

    @Test fun getStatusSoloAVersionesPosterioresALa1222() {
        assertEquals(Triple(1, 22, 2), RetroProtocol.parseVersion("1.22.2"))
        assertEquals(Triple(1, 23, 0), RetroProtocol.parseVersion("1.23"))
        assertEquals(Triple(1, 22, 3), RetroProtocol.parseVersion("1.22.3-dev\n"))
        assertNull(RetroProtocol.parseVersion("nightly"))
        for (v in listOf("1.22.2", "1.22.1", "1.21.0", "1.9.0", "1.22", "nightly", "")) assertFalse(v, RetroProtocol.statusQuerySafe(v))
        for (v in listOf("1.22.3", "1.23.0", "1.23", "2.0.0", "1.22.10")) assertTrue(v, RetroProtocol.statusQuerySafe(v))
        assertEquals(RetroProtocol.CMD_VERSION, RetroLink.probeFor(0, false, false))
        assertEquals(RetroProtocol.CMD_GET_STATUS, RetroLink.probeFor(0, false, true))
        assertEquals(RetroProtocol.CMD_VERSION, RetroLink.probeFor(0, true, true))
        assertEquals(RetroProtocol.CMD_VERSION, RetroLink.probeFor(1, false, true))
        assertEquals(RetroProtocol.CMD_GET_STATUS, RetroLink.probeFor(RetroLink.STATUS_EVERY * 3, false, true))
    }

    @Test fun lasTeclasRapidasTienenNombresUnicosYComandosDeRetroarch() {
        val names = mutableSetOf<String>()
        for (h in RetroProtocol.HOTKEYS) {
            assertTrue("repetida: ${h.name}", names.add(h.name))
            assertTrue(h.command.all { it.isUpperCase() || it == '_' })
        }
        assertEquals(29, RetroProtocol.HOTKEYS.size)
        assertEquals("FAST_FORWARD_HOLD", RetroProtocol.hotkey("fast_forward")!!.command)
        assertTrue(RetroProtocol.hotkey("rewind")!!.hold)
        assertFalse(RetroProtocol.hotkey("save_state")!!.hold)
        assertNull(RetroProtocol.hotkey("nada"))
    }

    // mapping.rs
    private fun packet(buttons: Int, flags: Int = 0, sx: Int = 0, sy: Int = 0, rx: Int = 0, ry: Int = 0) =
        ReceiverInput(1, 1, 0, 0f, 0f, 0f, 0f, 0f, 0f, buttons, 0, 50, sx, sy, rx, ry, flags)

    private fun bit(b: Int) = 1 shl b

    @Test fun elMandoApaisadoEsUnaSnesConGatillos() {
        val all = RetroMapping.BTN_A or RetroMapping.BTN_B or RetroMapping.BTN_X or RetroMapping.BTN_Y or RetroMapping.BTN_L or
            RetroMapping.BTN_R or RetroMapping.BTN_ZL or RetroMapping.BTN_ZR or RetroMapping.BTN_STICK_L or RetroMapping.BTN_STICK_R or
            RetroMapping.BTN_PLUS or RetroMapping.BTN_MINUS or RetroMapping.BTN_DPAD_UP or RetroMapping.BTN_DPAD_DOWN or
            RetroMapping.BTN_DPAD_LEFT or RetroMapping.BTN_DPAD_RIGHT
        assertEquals(0xffff, RetroMapping.padState(RetroPadKind.RetroPad, packet(all)).buttons)
        val s = RetroMapping.padState(RetroPadKind.RetroPad, packet(RetroMapping.BTN_A or RetroMapping.BTN_ZL or RetroMapping.BTN_MINUS))
        assertEquals(bit(RetroPad.A) or bit(RetroPad.L2) or bit(RetroPad.SELECT), s.buttons)
        // Home, Capturar, 1, 2 y los bits de sensor no son botones del RetroPad
        val others = RetroMapping.BTN_HOME or RetroMapping.BTN_SCREEN or RetroMapping.BTN_ONE or RetroMapping.BTN_TWO or (1 shl 17) or (1 shl 18) or (1 shl 27) or (1 shl 29) or (1 shl 30)
        assertEquals(0, RetroMapping.padState(RetroPadKind.RetroPad, packet(others)).buttons)
    }

    @Test fun losSticksSoloConSuBanderaYConYInvertida() {
        assertEquals(PadState.ZERO_AXES, RetroMapping.padState(RetroPadKind.RetroPad, packet(0, 0, 127, 127, -64, -127)).axes)
        assertEquals(listOf(32767, -32767, 0, 0), RetroMapping.padState(RetroPadKind.RetroPad, packet(0, RetroMapping.FLAG_STICK_VALID, 127, 127, -64, -127)).axes)
        val a = RetroMapping.padState(RetroPadKind.RetroPad, packet(0, RetroMapping.FLAG_STICK_VALID or RetroMapping.FLAG_EXT, 127, 127, -64, -127)).axes
        assertEquals(32767, a[0]); assertEquals(-32767, a[1]); assertTrue(a[2] in -16599..-16001); assertEquals(32767, a[3])
        assertEquals(PadState.ZERO_AXES, RetroMapping.padState(RetroPadKind.Nes, packet(0, 6, 127, 127, 1, 1)).axes)
    }

    @Test fun elMandoWiiDeLadoEsUnMandoDeNes() {
        val s = RetroMapping.padState(RetroPadKind.Nes, packet(RetroMapping.BTN_ONE or RetroMapping.BTN_DPAD_RIGHT or RetroMapping.BTN_PLUS))
        assertEquals(bit(RetroPad.B) or bit(RetroPad.RIGHT) or bit(RetroPad.START), s.buttons)
        val t = RetroMapping.padState(RetroPadKind.Nes, packet(RetroMapping.BTN_TWO or RetroMapping.BTN_A or RetroMapping.BTN_B or RetroMapping.BTN_MINUS))
        assertEquals(bit(RetroPad.A) or bit(RetroPad.X) or bit(RetroPad.Y) or bit(RetroPad.SELECT), t.buttons)
        assertEquals(0, RetroMapping.padState(RetroPadKind.Nes, packet(RetroMapping.BTN_X or RetroMapping.BTN_ZL or RetroMapping.BTN_STICK_L)).buttons)
        for (k in RetroPadKind.entries) assertEquals(k, RetroPadKind.parse(k.wire))
        assertNull(RetroPadKind.parse("pro"))
    }
}
