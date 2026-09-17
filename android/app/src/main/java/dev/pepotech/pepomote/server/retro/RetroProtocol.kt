package dev.pepotech.pepomote.server.retro

import java.nio.ByteBuffer
import java.nio.ByteOrder

/**
 * Lo que RetroArch entiende por el cable, igual que `desktop/src/retroarch/protocol.rs`:
 *
 * - **Mando en red** (Network Gamepad / RetroPad en red): UDP al puerto base +
 *   jugador (55400, 55401…). Cada datagrama es UN control: `struct remote_message
 *   { int port; int device; int index; int id; uint16_t state; }` = 20 bytes en
 *   little-endian. `device` 1 = botón del RetroPad (`id` 0..15), 5 = analógico
 *   (`index` 0 = stick izquierdo, 1 = derecho; `id` 0 = X, 1 = Y; `state` int16,
 *   +Y = abajo). RetroArch lee UN datagrama por jugador y por fotograma.
 * - **Interfaz de comandos**: UDP al 55355, texto. `VERSION` y `GET_STATUS`
 *   responden al remitente; las teclas rápidas (`MENU_TOGGLE`, `SAVE_STATE`…)
 *   valen un fotograma; las de mantener (`FAST_FORWARD_HOLD`, `REWIND`,
 *   `SLOWMOTION_HOLD`) hay que repetirlas cada fotograma.
 */
object RetroProtocol {
    /** `DEFAULT_NETWORK_REMOTE_BASE_PORT` (config.def.h). */
    const val DEFAULT_BASE_PORT = 55400
    /** `DEFAULT_NETWORK_CMD_PORT` (config.def.h). */
    const val DEFAULT_CMD_PORT = 55355
    /** `sizeof(struct remote_message)`: 4 enteros + uint16 + 2 de relleno. */
    const val MESSAGE_LEN = 20
    private const val DEVICE_JOYPAD = 1
    private const val DEVICE_ANALOG = 5

    /** 16 botones + 4 ejes. */
    const val CONTROLS = 20

    const val CMD_VERSION = "VERSION"
    const val CMD_GET_STATUS = "GET_STATUS"
    /** `SHOW_MSG <texto>`: aviso en la pantalla de RetroArch (180 fotogramas). */
    const val CMD_SHOW_MSG = "SHOW_MSG"

    /**
     * `GET_STATUS` cierra RetroArch 1.22.2 (y anteriores) cuando el núcleo cargado
     * no está en su lista de información: `command_get_status` copia un puntero
     * nulo. Corregido en master el 6-ene-2026, pero las nightlies siguen diciendo
     * «1.22.2»: el estado solo se pregunta a versiones estrictamente mayores.
     */
    val STATUS_UNSAFE_UPTO = Triple(1, 22, 2)

    /** El datagrama de un control: `remote_message` en little-endian. */
    fun encode(port: Int, control: Control, state: Int): ByteArray {
        val device: Int; val index: Int; val id: Int
        if (control.isButton) { device = DEVICE_JOYPAD; index = 0; id = control.id }
        else { device = DEVICE_ANALOG; index = control.id / 2; id = control.id % 2 }
        return ByteBuffer.allocate(MESSAGE_LEN).order(ByteOrder.LITTLE_ENDIAN)
            .putInt(port).putInt(device).putInt(index).putInt(id).putShort(state.toShort()).array()
    }

    /** Valor del control `c` en `s` tal como viaja en `state`. */
    fun value(s: PadState, c: Control): Int = if (c.isButton) (if (s.button(c.id)) 1 else 0) else s.axes[c.id]

    /** Stick del móvil (−127..127, +Y arriba) → eje de libretro (int16, +Y abajo). */
    fun axisFromStick(v: Int, invert: Boolean): Int {
        val clamped = v.coerceIn(-127, 127)
        val signed = if (invert) -clamped else clamped
        return signed * 32767 / 127
    }

    /** «1.22.2» → (1, 22, 2); «1.23» → (1, 23, 0). Ignora sufijos («-dev»). */
    fun parseVersion(text: String): Triple<Int, Int, Int>? {
        val parts = text.trim().split('.')
        fun num(i: Int): Int? = parts.getOrNull(i)?.takeWhile { it.isDigit() }?.toIntOrNull()
        val major = num(0) ?: return null
        return Triple(major, num(1) ?: 0, num(2) ?: 0)
    }

    private fun Triple<Int, Int, Int>.greaterThan(o: Triple<Int, Int, Int>): Boolean =
        first > o.first || (first == o.first && (second > o.second || (second == o.second && third > o.third)))

    /** ¿Se le puede preguntar el estado sin cerrarlo? */
    fun statusQuerySafe(version: String): Boolean = parseVersion(version)?.greaterThan(STATUS_UNSAFE_UPTO) == true

    /**
     * `GET_STATUS PLAYING core,contenido` / `GET_STATUS PAUSED …` / `GET_STATUS
     * CONTENTLESS`; cualquier otra línea corta sin espacios que empiece por un
     * dígito es la versión (`1.22.2`).
     */
    fun parseReply(text: String): Reply {
        val line = text.lineSequence().firstOrNull()?.trim().orEmpty()
        if (line.startsWith("GET_STATUS")) {
            val rest = line.removePrefix("GET_STATUS").trim()
            val space = rest.indexOf(' ')
            val state = if (space < 0) rest else rest.substring(0, space)
            val detail = if (space < 0) "" else rest.substring(space + 1).trim()
            val comma = detail.indexOf(',')
            val core = (if (comma < 0) detail else detail.substring(0, comma)).trim()
            var content = (if (comma < 0) "" else detail.substring(comma + 1)).trim()
            val crc = content.lastIndexOf(",crc32=")
            if (crc >= 0) content = content.substring(0, crc)
            return Reply.Status(when (state) {
                "PLAYING" -> Activity.Playing(core, content)
                "PAUSED" -> Activity.Paused(core, content)
                else -> Activity.Contentless
            })
        }
        if (line.isNotEmpty() && ' ' !in line && line.first().isDigit()) return Reply.Version(line)
        return Reply.Other(line)
    }

    /**
     * Tabla completa de teclas rápidas (`map[]` de command.h) que el móvil puede
     * pedir por nombre (mensaje `hotkey`, PROTOCOL.md §3).
     */
    val HOTKEYS: List<Hotkey> = listOf(
        Hotkey("menu", "MENU_TOGGLE"),
        Hotkey("save_state", "SAVE_STATE"),
        Hotkey("load_state", "LOAD_STATE"),
        Hotkey("slot_plus", "STATE_SLOT_PLUS"),
        Hotkey("slot_minus", "STATE_SLOT_MINUS"),
        Hotkey("fast_forward", "FAST_FORWARD_HOLD", hold = true),
        Hotkey("fast_forward_toggle", "FAST_FORWARD"),
        Hotkey("rewind", "REWIND", hold = true),
        Hotkey("slow_motion", "SLOWMOTION_HOLD", hold = true),
        Hotkey("pause", "PAUSE_TOGGLE"),
        Hotkey("frame_advance", "FRAMEADVANCE"),
        Hotkey("reset", "RESET"),
        Hotkey("screenshot", "SCREENSHOT"),
        Hotkey("fullscreen", "FULLSCREEN_TOGGLE"),
        Hotkey("mute", "MUTE"),
        Hotkey("volume_up", "VOLUME_UP"),
        Hotkey("volume_down", "VOLUME_DOWN"),
        Hotkey("close_content", "CLOSE_CONTENT"),
        Hotkey("quit", "QUIT"),
        Hotkey("shader_next", "SHADER_NEXT"),
        Hotkey("shader_prev", "SHADER_PREV"),
        Hotkey("shader_toggle", "SHADER_TOGGLE"),
        Hotkey("disk_eject", "DISK_EJECT_TOGGLE"),
        Hotkey("disk_next", "DISK_NEXT"),
        Hotkey("disk_prev", "DISK_PREV"),
        Hotkey("osk", "OSK"),
        Hotkey("game_focus", "GAME_FOCUS_TOGGLE"),
        Hotkey("grab_mouse", "GRAB_MOUSE_TOGGLE"),
        Hotkey("turbo_fire", "TURBO_FIRE_TOGGLE"),
    )

    fun hotkey(name: String): Hotkey? = HOTKEYS.firstOrNull { it.name == name }
}

/** Botones del RetroPad: `RETRO_DEVICE_ID_JOYPAD_*` (libretro.h), el orden de la SNES. */
object RetroPad {
    const val B = 0; const val Y = 1; const val SELECT = 2; const val START = 3
    const val UP = 4; const val DOWN = 5; const val LEFT = 6; const val RIGHT = 7
    const val A = 8; const val X = 9; const val L = 10; const val R = 11
    const val L2 = 12; const val R2 = 13; const val L3 = 14; const val R3 = 15
}

/** Botones y ejes de un jugador tal como los guarda RetroArch: 16 bits y cuatro int16 (LX, LY, RX, RY). */
data class PadState(val buttons: Int = 0, val axes: List<Int> = ZERO_AXES) {
    fun button(id: Int): Boolean = buttons and (1 shl id) != 0
    val isZero: Boolean get() = buttons == 0 && axes.all { it == 0 }

    companion object { val ZERO_AXES: List<Int> = listOf(0, 0, 0, 0) }
}

/** Un control del mando remoto: índice 0..15 = botón, 16..19 = eje. */
data class Control(val index: Int) {
    val isButton: Boolean get() = index < 16
    val id: Int get() = if (isButton) index else index - 16

    companion object {
        fun button(id: Int) = Control(id)
        fun axis(id: Int) = Control(16 + id)
    }
}

/** Nombre del protocolo, comando de RetroArch y si es de mantener (se repite cada fotograma). */
data class Hotkey(val name: String, val command: String, val hold: Boolean = false)

sealed class Activity {
    object Contentless : Activity()
    data class Playing(val core: String, val content: String) : Activity()
    data class Paused(val core: String, val content: String) : Activity()
}

sealed class Reply {
    data class Version(val version: String) : Reply()
    data class Status(val activity: Activity) : Reply()
    data class Other(val line: String) : Reply()
}
