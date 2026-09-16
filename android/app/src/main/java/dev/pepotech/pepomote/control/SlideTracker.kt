package dev.pepotech.pepomote.control

/** Lo que hay que hacerle a un bit por culpa de un dedo. */
sealed interface PressEvent {
    data class Press(val bit: Int) : PressEvent
    data class Release(val bit: Int) : PressEvent
}

/**
 * La máquina de dedos de la capa de pulsación (`slideCanvas` en Compose):
 * cada dedo lleva su propia cadena (el bit que tiene cogido, o ninguno) y
 * [PressStep.next] decide qué pasa al moverlo. Aparte de Compose aposta: así
 * se prueba en la JVM (dos dedos a la vez, barridos, dedos descartados).
 *
 * [hit]: qué botón hay en un punto ([PressHit.resolve] sobre las zonas
 * registradas). Un dedo cuyo `down` ya se comió otro control (stick,
 * cruceta, chips, diana, táctil) se descarta entero con [skip]: no pulsa
 * nada aunque luego pase por encima de un botón.
 */
class SlideTracker(private val hit: (Float, Float) -> Int?) {
    /** Dedo → bit que lleva (null = apoyado pero sin nada). */
    private val fingers = HashMap<Long, Int?>()
    private val skipped = HashSet<Long>()

    /**
     * Este dedo no es nuestro: alguien se comió su `down` (o, más tarde, su
     * arrastre: la tira de scroll solo lo hace al empezar a arrastrar).
     * Suelta lo que llevara y no vuelve a coger nada hasta levantarlo.
     */
    fun skip(id: Long): List<PressEvent> {
        val held = fingers.remove(id)
        skipped.add(id)
        return if (held != null) listOf(PressEvent.Release(held)) else emptyList()
    }

    /** Dedo nuevo en ([x], [y]). Solo deslizando coge algo al nacer. */
    fun down(id: Long, x: Float, y: Float, slide: Boolean, sticky: Boolean): List<PressEvent> {
        skipped.remove(id)
        fingers[id] = null
        return step(id, x, y, slide, sticky)
    }

    /** El dedo se ha movido a ([x], [y]). */
    fun move(id: Long, x: Float, y: Float, slide: Boolean, sticky: Boolean): List<PressEvent> {
        if (id in skipped || id !in fingers) return emptyList()
        return step(id, x, y, slide, sticky)
    }

    /** Dedo levantado: suelta lo que llevara. */
    fun up(id: Long): List<PressEvent> {
        skipped.remove(id)
        val held = fingers.remove(id)
        return if (held != null) listOf(PressEvent.Release(held)) else emptyList()
    }

    /** Se acabó (se sale de la pantalla, se cancela el gesto): todo suelto. */
    fun releaseAll(): List<PressEvent> {
        val out = fingers.values.filterNotNull().distinct().map { PressEvent.Release(it) }
        fingers.clear()
        skipped.clear()
        return out
    }

    private fun step(id: Long, x: Float, y: Float, slide: Boolean, sticky: Boolean): List<PressEvent> {
        val held = fingers[id]
        // Un botón que ya lleva otro dedo no se le quita a este: si no, al
        // levantar el segundo se soltaría el botón que el primero sigue apretando
        val over = hit(x, y)?.takeIf { bit -> bit == held || fingers.none { (f, b) -> f != id && b == bit } }
        return when (val move = PressStep.next(slide, sticky, held, over)) {
            is PressStep.Move.Keep -> emptyList()
            is PressStep.Move.To -> {
                fingers[id] = move.bit
                buildList {
                    if (held != null) add(PressEvent.Release(held))
                    if (move.bit != null) add(PressEvent.Press(move.bit))
                }
            }
        }
    }
}
