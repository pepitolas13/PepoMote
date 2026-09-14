package dev.pepotech.pepomote.service

import dev.pepotech.pepomote.sensor.Frame
import org.junit.Assert.assertEquals
import org.junit.Test

/** Lado del mando + Nunchuk apaisado: preferencia, orientación y lado visible. */
class LandscapeSideTest {

    @Test
    fun preferenciaIdaYVuelta() {
        for (s in LandscapeSide.values()) assertEquals(s, LandscapeSide.fromPref(s.pref))
        assertEquals(LandscapeSide.Unset, LandscapeSide.fromPref(""))
        assertEquals(LandscapeSide.Unset, LandscapeSide.fromPref("lo que sea"))
        assertEquals(LandscapeSide.Left, LandscapeSide.fromPref("left"))
        assertEquals(LandscapeSide.Sensor, LandscapeSide.fromPref("sensor"))
    }

    @Test
    fun orientacionYVuelta() {
        assertEquals(LandscapeOrientation.Landscape, LandscapeSide.Left.orientation)
        assertEquals(LandscapeOrientation.ReverseLandscape, LandscapeSide.Right.orientation)
        // Sin elegir, y con «según el sensor»: los dos apaisados, como siempre
        assertEquals(LandscapeOrientation.SensorLandscape, LandscapeSide.Unset.orientation)
        assertEquals(LandscapeOrientation.SensorLandscape, LandscapeSide.Sensor.orientation)
        assertEquals(LandscapeSide.Right, LandscapeSide.Left.flipped())
        assertEquals(LandscapeSide.Left, LandscapeSide.Right.flipped())
        assertEquals(LandscapeSide.Unset, LandscapeSide.Unset.flipped())
        assertEquals(LandscapeSide.Sensor, LandscapeSide.Sensor.flipped())
    }

    @Test
    fun ladoVisibleSegunLaRotacionDeLaPantalla() {
        // Móvil (vertical de fábrica): 90 = borde superior a la izquierda
        assertEquals(LandscapeSide.Left, LandscapeSide.current(Frame.ROTATION_90))
        assertEquals(LandscapeSide.Right, LandscapeSide.current(Frame.ROTATION_270))
        // Tablet apaisada de fábrica
        assertEquals(LandscapeSide.Left, LandscapeSide.current(Frame.ROTATION_0))
        assertEquals(LandscapeSide.Right, LandscapeSide.current(Frame.ROTATION_180))
    }

    @Test
    fun elProvisionalManda() {
        assertEquals(LandscapeSide.Unset, LandscapeSide.effective(LandscapeSide.Unset, null))
        assertEquals(LandscapeSide.Right, LandscapeSide.effective(LandscapeSide.Unset, LandscapeSide.Right))
        assertEquals(LandscapeSide.Left, LandscapeSide.effective(LandscapeSide.Left, null))
        assertEquals(LandscapeSide.Right, LandscapeSide.effective(LandscapeSide.Left, LandscapeSide.Right))
    }
}
