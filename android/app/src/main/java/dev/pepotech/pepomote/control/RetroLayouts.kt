package dev.pepotech.pepomote.control

/**
 * Plantillas de mando de RetroArch por consola: qué botones enseña el móvil,
 * con qué etiqueta y color, y qué bit del INPUT emite cada uno. El receptor
 * traduce los bits al RetroPad como siempre (A→A, B→B, X→X, Y→Y, L→L, R→R,
 * ZL→L2, ZR→R2, clics→L3/R3, +→Start, −→Select); cada botón en pantalla
 * emite el bit cuyo id del RetroPad espera el mapeo por defecto del núcleo.
 * Fijos en toda plantilla y fuera de la tabla: la cruceta, «Menú» (HOME) y
 * «Rápido» (SCREEN).
 *
 * Copia a mano de `pmp::retro` (la fuente de verdad, en Rust); el test de
 * paridad la compara con `protocol/retro-layouts.json`.
 */
enum class RetroShape(val id: String) {
    /** Un botón: Atari 2600. */
    SINGLE("single"),
    /** Dos en fila (`left`, `right`), con `stagger`: NES, Game Boy, Master System, PC Engine, N64. */
    TWO("two"),
    /** Rombo `top`/`left`/`right`/`bottom`: RetroPad, SNES, Neo Geo, PlayStation. */
    DIAMOND("diamond"),
    /** Dos filas de tres `tl tm tr` / `bl bm br`: Mega Drive de 6 botones, arcade. */
    GRID2X3("grid2x3"),
    /** Tres en fila `l m r`: Mega Drive de 3 botones. */
    ROW3("row3");

    val slots: List<String>
        get() = when (this) {
            SINGLE -> listOf("center")
            TWO -> listOf("left", "right")
            DIAMOND -> listOf("top", "left", "right", "bottom")
            GRID2X3 -> listOf("tl", "tm", "tr", "bl", "bm", "br")
            ROW3 -> listOf("l", "m", "r")
        }
}

/** Qué va en el hueco del stick derecho. */
enum class RightStick(val id: String) {
    NONE("none"),
    ANALOG("analog"),
    /** N64: cuatro botones C que emiten los ejes del stick derecho a ±127. */
    CBUTTONS("cbuttons")
}

/** Color del botón; cada app lo lleva a su tema (`CARD` = el de siempre). */
enum class RetroColor(val id: String) {
    CARD("card"), RED("red"), YELLOW("yellow"), GREEN("green"), BLUE("blue"), PINK("pink"), PURPLE("purple")
}

/**
 * Un botón frontal. [label] es el texto tal cual salvo `$coin` y `$fire`, que
 * se traducen («Moneda», «Disparo»). [primary]: el azul destacado de siempre
 * (con «pop»). [size]: relativo al botón normal.
 */
data class RetroFace(
    val slot: String,
    val label: String,
    val bit: Int,
    val color: RetroColor = RetroColor.CARD,
    val primary: Boolean = false,
    val size: Float = 1f
)

/** Un botón de la fila central (Select/Start y sus nombres en cada consola). */
data class RetroCenter(val label: String, val bit: Int)

/** Etiquetas de los hombros; null = ese hombro no existe en la consola. */
data class RetroShoulders(val l: String?, val r: String?, val l2: String?, val r2: String?)

data class RetroLayout(
    val id: String,
    /** Nombre propio, igual en los dos idiomas. */
    val name: String,
    val shape: RetroShape,
    /** Solo TWO: −1 el hueco izquierdo más alto (N64), 0 a nivel, 1 más bajo (Game Boy). */
    val stagger: Int,
    val face: List<RetroFace>,
    val leftStick: Boolean,
    val rightStick: RightStick,
    /** L3/R3 (solo con sticks analógicos). */
    val stickClicks: Boolean,
    val shoulders: RetroShoulders,
    val center: List<RetroCenter>
)

object RetroLayouts {
    private val NO_SHOULDERS = RetroShoulders(null, null, null, null)
    private val LR = RetroShoulders("L", "R", null, null)
    private val SELECT_START = listOf(RetroCenter("Select", ButtonState.MINUS), RetroCenter("Start", ButtonState.PLUS))
    private val MODE_START = listOf(RetroCenter("Mode", ButtonState.MINUS), RetroCenter("Start", ButtonState.PLUS))
    private val COIN_START = listOf(RetroCenter("\$coin", ButtonState.MINUS), RetroCenter("Start", ButtonState.PLUS))

    /** El RetroPad completo: lo de hoy, y lo que se enseña cuando no se sabe la consola. */
    val RETROPAD = RetroLayout(
        id = "retropad", name = "RetroPad", shape = RetroShape.DIAMOND, stagger = 0,
        face = listOf(
            RetroFace("top", "X", ButtonState.X),
            RetroFace("left", "Y", ButtonState.Y),
            RetroFace("right", "A", ButtonState.A, primary = true),
            RetroFace("bottom", "B", ButtonState.B)
        ),
        leftStick = true, rightStick = RightStick.ANALOG, stickClicks = true,
        shoulders = RetroShoulders("L", "R", "L2", "R2"), center = SELECT_START
    )

    /** Todas las plantillas, con `retropad` la primera (mismo orden que el JSON). */
    val ALL: List<RetroLayout> = listOf(
        RETROPAD,
        // FCEUmm / Nestopia: A→A, B→B
        RetroLayout(
            "nes", "NES", RetroShape.TWO, 0,
            listOf(RetroFace("left", "B", ButtonState.B, RetroColor.RED), RetroFace("right", "A", ButtonState.A, RetroColor.RED, primary = true)),
            false, RightStick.NONE, false, NO_SHOULDERS, SELECT_START
        ),
        // Gambatte / SameBoy: A→A, B→B
        RetroLayout(
            "gb", "Game Boy", RetroShape.TWO, 1,
            listOf(RetroFace("left", "B", ButtonState.B, RetroColor.PURPLE), RetroFace("right", "A", ButtonState.A, RetroColor.PURPLE, primary = true)),
            false, RightStick.NONE, false, NO_SHOULDERS, SELECT_START
        ),
        // mGBA: A→A, B→B, L→L, R→R
        RetroLayout(
            "gba", "Game Boy Advance", RetroShape.TWO, 1,
            listOf(RetroFace("left", "B", ButtonState.B), RetroFace("right", "A", ButtonState.A, primary = true)),
            false, RightStick.NONE, false, LR, SELECT_START
        ),
        // Snes9x: identidad (colores del mando PAL/Super Famicom)
        RetroLayout(
            "snes", "SNES", RetroShape.DIAMOND, 0,
            listOf(
                RetroFace("top", "X", ButtonState.X, RetroColor.BLUE),
                RetroFace("left", "Y", ButtonState.Y, RetroColor.GREEN),
                RetroFace("right", "A", ButtonState.A, RetroColor.RED, primary = true),
                RetroFace("bottom", "B", ButtonState.B, RetroColor.YELLOW)
            ),
            false, RightStick.NONE, false, LR, SELECT_START
        ),
        // Genesis Plus GX / PicoDrive (Master System, Game Gear): 1→B, 2→A, Pause→Start
        RetroLayout(
            "ms", "Master System", RetroShape.TWO, 0,
            listOf(RetroFace("left", "1", ButtonState.B, primary = true), RetroFace("right", "2", ButtonState.A)),
            false, RightStick.NONE, false, NO_SHOULDERS, listOf(RetroCenter("Pause", ButtonState.PLUS))
        ),
        // Genesis Plus GX / PicoDrive: A→Y, B→B, C→A, X→L, Y→X, Z→R, Mode→Select
        RetroLayout(
            "md", "Mega Drive", RetroShape.GRID2X3, 0,
            listOf(
                RetroFace("tl", "X", ButtonState.L, size = 0.8f),
                RetroFace("tm", "Y", ButtonState.X, size = 0.8f),
                RetroFace("tr", "Z", ButtonState.R, size = 0.8f),
                RetroFace("bl", "A", ButtonState.Y),
                RetroFace("bm", "B", ButtonState.B),
                RetroFace("br", "C", ButtonState.A, primary = true)
            ),
            false, RightStick.NONE, false, NO_SHOULDERS, MODE_START
        ),
        // El mando de 3 botones, solo a mano
        RetroLayout(
            "md3", "Mega Drive (3)", RetroShape.ROW3, 0,
            listOf(RetroFace("l", "A", ButtonState.Y), RetroFace("m", "B", ButtonState.B), RetroFace("r", "C", ButtonState.A, primary = true)),
            false, RightStick.NONE, false, NO_SHOULDERS, MODE_START
        ),
        // Beetle PCE: I→A, II→B, Run→Start
        RetroLayout(
            "pce", "PC Engine", RetroShape.TWO, 0,
            listOf(RetroFace("left", "II", ButtonState.B), RetroFace("right", "I", ButtonState.A, primary = true)),
            false, RightStick.NONE, false, NO_SHOULDERS, listOf(RetroCenter("Select", ButtonState.MINUS), RetroCenter("Run", ButtonState.PLUS))
        ),
        // FBNeo («Classic») y MAME: 1..6 → B A Y X R L; en lucha LP MP HP = Y X L, LK MK HK = B A R
        RetroLayout(
            "arcade", "Arcade", RetroShape.GRID2X3, 0,
            listOf(
                RetroFace("tl", "Y", ButtonState.Y), RetroFace("tm", "X", ButtonState.X), RetroFace("tr", "L", ButtonState.L),
                RetroFace("bl", "B", ButtonState.B, primary = true), RetroFace("bm", "A", ButtonState.A), RetroFace("br", "R", ButtonState.R)
            ),
            false, RightStick.NONE, false, NO_SHOULDERS, COIN_START
        ),
        // FBNeo Neo Geo («Classic»): A→B, B→A, C→Y, D→X, en el rombo del mando de Neo Geo CD
        RetroLayout(
            "neogeo", "Neo Geo", RetroShape.DIAMOND, 0,
            listOf(
                RetroFace("top", "D", ButtonState.X, RetroColor.BLUE),
                RetroFace("left", "C", ButtonState.Y, RetroColor.GREEN),
                RetroFace("right", "B", ButtonState.A, RetroColor.YELLOW),
                RetroFace("bottom", "A", ButtonState.B, RetroColor.RED, primary = true)
            ),
            false, RightStick.NONE, false, NO_SHOULDERS, COIN_START
        ),
        // Stella: disparo→B, Select→Select, Reset→Start
        RetroLayout(
            "atari2600", "Atari 2600", RetroShape.SINGLE, 0,
            listOf(RetroFace("center", "\$fire", ButtonState.B, RetroColor.RED, primary = true)),
            false, RightStick.NONE, false, NO_SHOULDERS, listOf(RetroCenter("Select", ButtonState.MINUS), RetroCenter("Reset", ButtonState.PLUS))
        ),
        // Mupen64Plus: A→B, B→Y, Z→L2, L→L, R→R; botones C = stick derecho
        RetroLayout(
            "n64", "Nintendo 64", RetroShape.TWO, -1,
            listOf(RetroFace("left", "B", ButtonState.Y, RetroColor.GREEN, size = 0.9f), RetroFace("right", "A", ButtonState.B, RetroColor.BLUE, primary = true, size = 1.2f)),
            true, RightStick.CBUTTONS, false, RetroShoulders("L", "R", "Z", null), listOf(RetroCenter("Start", ButtonState.PLUS))
        ),
        // PCSX-ReARMed / Beetle PSX: ✕→B, ○→A, □→Y, △→X, L1/R1/L2/R2, L3/R3
        RetroLayout(
            "psx", "PlayStation", RetroShape.DIAMOND, 0,
            listOf(
                RetroFace("top", "△", ButtonState.X, RetroColor.GREEN),
                RetroFace("left", "□", ButtonState.Y, RetroColor.PINK),
                RetroFace("right", "○", ButtonState.A, RetroColor.RED),
                RetroFace("bottom", "✕", ButtonState.B, RetroColor.BLUE, primary = true)
            ),
            true, RightStick.ANALOG, true, RetroShoulders("L1", "R1", "L2", "R2"), SELECT_START
        )
    )

    /** Las consolas que el receptor puede anunciar en `game.console`. */
    val CONSOLE_IDS = listOf("nes", "gb", "gba", "snes", "ms", "md", "pce", "arcade", "neogeo", "atari2600", "n64", "psx")

    /** Ids válidos en `pad.layout` (todas las plantillas). */
    val LAYOUT_IDS: List<String> = ALL.map { it.id }

    fun byId(id: String?): RetroLayout? = ALL.firstOrNull { it.id == id }

    fun isConsole(id: String?): Boolean = id in CONSOLE_IDS

    /**
     * Qué plantilla se enseña: la elegida a mano si es válida, si no la consola
     * que anunció el receptor, si no el RetroPad completo (igual que en Rust).
     */
    fun effective(console: String?, chosen: String?): String = when {
        chosen != null && chosen in LAYOUT_IDS -> chosen
        isConsole(console) -> console!!
        else -> RETROPAD.id
    }

    /** Nombre del bit para el JSON de paridad. */
    val BIT_NAMES: List<Pair<String, Int>> = listOf(
        "A" to ButtonState.A, "B" to ButtonState.B, "X" to ButtonState.X, "Y" to ButtonState.Y,
        "L" to ButtonState.L, "R" to ButtonState.R, "ZL" to ButtonState.ZL, "ZR" to ButtonState.ZR,
        "STICK_L" to ButtonState.STICK_L, "STICK_R" to ButtonState.STICK_R,
        "PLUS" to ButtonState.PLUS, "MINUS" to ButtonState.MINUS
    )

    fun bitName(bit: Int): String? = BIT_NAMES.firstOrNull { it.second == bit }?.first
}
