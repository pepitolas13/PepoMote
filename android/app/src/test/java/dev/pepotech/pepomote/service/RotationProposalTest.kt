package dev.pepotech.pepomote.service

import android.content.pm.ActivityInfo
import android.view.Surface
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** Propuesta de girar la pantalla: clasificación con histéresis, orientación a pedir y reloj. */
class RotationProposalTest {
    @Test fun facingWithHysteresis() {
        assertEquals(Facing.Portrait, RotationProposal.facing(0, Facing.Other))
        assertEquals(Facing.Portrait, RotationProposal.facing(25, Facing.Other))
        assertEquals(Facing.Portrait, RotationProposal.facing(340, Facing.Other))
        assertEquals("entre bandas se conserva la anterior", Facing.Portrait, RotationProposal.facing(45, Facing.Portrait))
        assertEquals(Facing.LandscapeRight, RotationProposal.facing(45, Facing.LandscapeRight))
        assertEquals(Facing.LandscapeRight, RotationProposal.facing(70, Facing.Portrait))
        assertEquals(Facing.LandscapeRight, RotationProposal.facing(110, Facing.Portrait))
        assertEquals(Facing.LandscapeLeft, RotationProposal.facing(250, Facing.Portrait))
        assertEquals(Facing.LandscapeLeft, RotationProposal.facing(290, Facing.Portrait))
        assertEquals("boca abajo no se propone", Facing.Other, RotationProposal.facing(180, Facing.Portrait))
        assertEquals("plano/desconocido conserva la anterior", Facing.LandscapeLeft, RotationProposal.facing(-1, Facing.LandscapeLeft))
        assertEquals(Facing.Portrait, RotationProposal.facing(360, Facing.Other))
    }

    @Test fun proposalOnlyWhenScreenAndPhoneDisagree() {
        assertNull(RotationProposal.proposal(Facing.Portrait, Surface.ROTATION_0))
        assertEquals(ActivityInfo.SCREEN_ORIENTATION_PORTRAIT, RotationProposal.proposal(Facing.Portrait, Surface.ROTATION_90))
        assertEquals(ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE, RotationProposal.proposal(Facing.LandscapeLeft, Surface.ROTATION_0))
        assertEquals(ActivityInfo.SCREEN_ORIENTATION_REVERSE_LANDSCAPE, RotationProposal.proposal(Facing.LandscapeRight, Surface.ROTATION_0))
        assertNull(RotationProposal.proposal(Facing.LandscapeLeft, Surface.ROTATION_90))
        assertNull(RotationProposal.proposal(Facing.LandscapeRight, Surface.ROTATION_270))
        assertEquals("de un apaisado al otro", ActivityInfo.SCREEN_ORIENTATION_REVERSE_LANDSCAPE, RotationProposal.proposal(Facing.LandscapeRight, Surface.ROTATION_90))
        assertNull(RotationProposal.proposal(Facing.Other, Surface.ROTATION_0))
        // tableta apaisada de fábrica: los nombres van cruzados
        assertEquals(ActivityInfo.SCREEN_ORIENTATION_PORTRAIT, RotationProposal.proposal(Facing.LandscapeLeft, Surface.ROTATION_0, naturalPortrait = false))
        assertEquals(ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE, RotationProposal.proposal(Facing.Portrait, Surface.ROTATION_90, naturalPortrait = false))
    }

    @Test fun machineProposesAfterSettlingAndExpiresUntilTheSideChanges() {
        val m = RotationProposalMachine()
        // derecho y pantalla derecha: nada
        assertNull(m.onDegrees(0, Surface.ROTATION_0, true, 0))
        // girado a la izquierda: hay que asentarse antes de proponer
        assertNull(m.onDegrees(270, Surface.ROTATION_0, true, 100))
        assertNull(m.onDegrees(272, Surface.ROTATION_0, true, 800))
        assertEquals(ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE, m.onDegrees(268, Surface.ROTATION_0, true, 100 + RotationProposalMachine.SETTLE_MS))
        // sigue en pantalla mientras no caduque
        assertEquals(ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE, m.onDegrees(270, Surface.ROTATION_0, true, 3000))
        // caduca sola y no vuelve aunque el móvil siga girado
        assertNull(m.onDegrees(270, Surface.ROTATION_0, true, 100 + RotationProposalMachine.SETTLE_MS + RotationProposalMachine.SHOW_MS))
        assertNull(m.onDegrees(270, Surface.ROTATION_0, true, 20_000))
        // el móvil vuelve a estar derecho: se olvida; y al girarlo otra vez vuelve a proponer
        assertNull(m.onDegrees(0, Surface.ROTATION_0, true, 21_000))
        assertNull(m.onDegrees(270, Surface.ROTATION_0, true, 22_000))
        assertEquals(ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE, m.onDegrees(270, Surface.ROTATION_0, true, 22_000 + RotationProposalMachine.SETTLE_MS))
        // aceptar la quita; con la pantalla ya girada no hay nada que proponer
        assertEquals(ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE, m.accept())
        assertNull(m.proposal)
        assertNull(m.onDegrees(270, Surface.ROTATION_90, true, 24_000))
        // y al ponerlo derecho, propone volver a vertical
        assertNull(m.onDegrees(0, Surface.ROTATION_90, true, 25_000))
        assertEquals(ActivityInfo.SCREEN_ORIENTATION_PORTRAIT, m.onDegrees(0, Surface.ROTATION_90, true, 25_000 + RotationProposalMachine.SETTLE_MS))
    }

    @Test fun machineSwitchesSidesAndRespectsDisabled() {
        val m = RotationProposalMachine()
        m.onDegrees(270, Surface.ROTATION_0, true, 0)
        assertEquals(ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE, m.onDegrees(270, Surface.ROTATION_0, true, RotationProposalMachine.SETTLE_MS))
        // gira al otro lado con la propuesta en pantalla: la nueva, tras asentarse
        assertNull(m.onDegrees(90, Surface.ROTATION_0, true, 2000))
        assertEquals(ActivityInfo.SCREEN_ORIENTATION_REVERSE_LANDSCAPE, m.onDegrees(90, Surface.ROTATION_0, true, 2000 + RotationProposalMachine.SETTLE_MS))
        // sin permiso (fuera del mando, o giro automático del sistema activo): nunca
        val d = RotationProposalMachine()
        assertNull(d.onDegrees(270, Surface.ROTATION_0, true, 0, enabled = false))
        assertNull(d.onDegrees(270, Surface.ROTATION_0, true, 5000, enabled = false))
        assertNull(d.accept())
        // al volver a permitirse, la cuenta empieza de cero
        assertNull(d.onDegrees(270, Surface.ROTATION_0, true, 5001))
        assertEquals(ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE, d.onDegrees(270, Surface.ROTATION_0, true, 5001 + RotationProposalMachine.SETTLE_MS))
        d.clear()
        assertNull(d.proposal)
    }
}
