package dev.pepotech.pepomote.server.retro

/**
 * Qué mando enseña el móvil para cada consola de RetroArch, igual que
 * `desktop/src/retroarch/consoles.rs`: del nombre del núcleo, de la extensión
 * del contenido en los núcleos multisistema (Genesis Plus GX juega Master
 * System y Mega Drive; mGBA juega Game Boy y Advance), del `db_name` del
 * historial y, si se conoce, del `systemid` de la ficha. Los ids son los de
 * `RetroLayouts.CONSOLE_IDS` (`protocol/retro-layouts.json`).
 */
enum class Console(val id: String, val displayName: String) {
    Nes("nes", "NES"),
    Gb("gb", "Game Boy"),
    Gba("gba", "Game Boy Advance"),
    Snes("snes", "SNES"),
    Ms("ms", "Master System"),
    Md("md", "Mega Drive"),
    Pce("pce", "PC Engine"),
    Arcade("arcade", "Arcade"),
    NeoGeo("neogeo", "Neo Geo"),
    Atari2600("atari2600", "Atari 2600"),
    N64("n64", "Nintendo 64"),
    Psx("psx", "PlayStation");

    companion object {
        fun parse(id: String?): Console? = entries.firstOrNull { it.id == id }
    }
}

object RetroConsoles {
    /** Nombre del archivo (tras la última barra) y, si la ruta lleva `#`, el del miembro del comprimido. */
    fun contentFileName(path: String): String {
        val afterHash = path.substringAfterLast('#')
        return afterHash.substringAfterLast('/').substringAfterLast('\\')
    }

    /** Extensión del contenido en minúsculas y sin punto. */
    fun contentExt(path: String): String? {
        val name = contentFileName(path)
        val dot = name.lastIndexOf('.')
        if (dot < 0) return null
        val ext = name.substring(dot + 1)
        if (ext.isEmpty() || ' ' in ext) return null
        return ext.lowercase()
    }

    /** Extensiones que solo pueden ser de una consola (las ambiguas, como `.bin`, `.zip` o `.cue`, no deciden). */
    private fun byExt(ext: String): Console? = when (ext) {
        "nes", "fds", "unf", "unif" -> Console.Nes
        "gb", "gbc", "dmg", "cgb", "sgb" -> Console.Gb
        "gba" -> Console.Gba
        "sfc", "smc", "swc", "fig", "bs", "st" -> Console.Snes
        "sms", "gg", "sg", "sc" -> Console.Ms
        "md", "smd", "gen", "mdx", "68k", "sgd", "32x" -> Console.Md
        "pce", "sgx" -> Console.Pce
        "a26" -> Console.Atari2600
        "n64", "v64", "z64", "ndd" -> Console.N64
        "neo" -> Console.NeoGeo
        else -> null
    }

    private fun bySystemId(id: String): Console? = when (id) {
        "nes" -> Console.Nes
        "game_boy" -> Console.Gb
        "game_boy_advance" -> Console.Gba
        "super_nes" -> Console.Snes
        "master_system" -> Console.Ms
        "mega_drive" -> Console.Md
        "pc_engine" -> Console.Pce
        "mame", "fb_alpha" -> Console.Arcade
        "neogeo", "neo_geo" -> Console.NeoGeo
        "atari_2600" -> Console.Atari2600
        "nintendo_64" -> Console.N64
        "playstation" -> Console.Psx
        else -> null
    }

    /** `genesis_plus_gx_libretro_android.so` → `genesis_plus_gx` (también `.dll`, `.dylib`), en minúsculas. */
    fun coreStem(corePath: String): String {
        var name = corePath.substringAfterLast('/').substringAfterLast('\\').lowercase()
        for (ext in listOf(".dll", ".so", ".dylib")) if (name.endsWith(ext)) { name = name.removeSuffix(ext); break }
        for (suffix in listOf("_libretro_android", "_libretro")) if (name.endsWith(suffix)) { name = name.removeSuffix(suffix); break }
        return name
    }

    private val PREFIXES: List<Pair<String, Console>> = listOf(
        "mesen-s" to Console.Snes, "mesen_s" to Console.Snes, "snes9x" to Console.Snes, "bsnes" to Console.Snes,
        "higan_sfc" to Console.Snes, "chimerasnes" to Console.Snes, "mednafen_snes" to Console.Snes,
        "beetle_snes" to Console.Snes, "supafaust" to Console.Snes, "mednafen_supafaust" to Console.Snes,
        "fceumm" to Console.Nes, "nestopia" to Console.Nes, "mesen" to Console.Nes, "quicknes" to Console.Nes,
        "bnes" to Console.Nes, "fixnes" to Console.Nes, "emux_nes" to Console.Nes,
        "gambatte" to Console.Gb, "sameboy" to Console.Gb, "tgbdual" to Console.Gb, "gearboy" to Console.Gb,
        "doublecherrygb" to Console.Gb, "boytacean" to Console.Gb, "fixgb" to Console.Gb, "emux_gb" to Console.Gb,
        "mgba" to Console.Gba, "vbam" to Console.Gba, "vba_next" to Console.Gba, "gpsp" to Console.Gba,
        "mednafen_gba" to Console.Gba, "beetle_gba" to Console.Gba, "meteor" to Console.Gba, "tempgba" to Console.Gba,
        "genesis_plus_gx" to Console.Md, "picodrive" to Console.Md, "blastem" to Console.Md, "clownmdemu" to Console.Md,
        "smsplus" to Console.Ms, "gearsystem" to Console.Ms, "emux_sms" to Console.Ms,
        "mednafen_pce" to Console.Pce, "beetle_pce" to Console.Pce, "mednafen_supergrafx" to Console.Pce,
        "beetle_supergrafx" to Console.Pce, "geargrafx" to Console.Pce,
        "mupen64plus" to Console.N64, "parallel_n64" to Console.N64,
        "pcsx" to Console.Psx, "swanstation" to Console.Psx, "duckstation" to Console.Psx,
        "mednafen_psx" to Console.Psx, "beetle_psx" to Console.Psx, "rustation" to Console.Psx,
        "stella" to Console.Atari2600,
        "fbneo" to Console.Arcade, "fbalpha" to Console.Arcade, "fba" to Console.Arcade, "mame" to Console.Arcade,
    )

    /** Núcleo sin ficha: por el nombre de su archivo (prefijos del más específico al más general). */
    fun byCoreName(coreFile: String): Console? {
        val stem = coreStem(coreFile)
        if ("neogeo" in stem || "neocd" in stem || "geolith" in stem) return Console.NeoGeo
        return PREFIXES.firstOrNull { (p, _) -> stem.startsWith(p) }?.second
    }

    private val HINTS: List<Pair<String, Console>> = listOf(
        "Game Boy Advance" to Console.Gba, "Game Boy" to Console.Gb,
        "Mega Drive" to Console.Md, "32X" to Console.Md, "Mega-CD" to Console.Md,
        "Master System" to Console.Ms, "Game Gear" to Console.Ms, "SG-1000" to Console.Ms,
        "Super Nintendo" to Console.Snes, "Satellaview" to Console.Snes, "Sufami" to Console.Snes,
        "Nintendo Entertainment System" to Console.Nes, "Family Computer" to Console.Nes,
        "PC Engine" to Console.Pce, "TurboGrafx" to Console.Pce, "SuperGrafx" to Console.Pce,
        "Neo Geo" to Console.NeoGeo, "FBNeo" to Console.Arcade, "MAME" to Console.Arcade, "Arcade" to Console.Arcade,
        "Atari - 2600" to Console.Atari2600, "Nintendo 64" to Console.N64, "PlayStation" to Console.Psx,
    )

    /** `db_name` del historial cuando es UNA base de datos (sin `|`): la pista más fiable con `.bin`/`.cue`. */
    fun byDbName(db: String): Console? {
        if (db.isEmpty() || '|' in db) return null
        return HINTS.firstOrNull { (h, _) -> h in db }?.second
    }

    /** Consola del contenido. Orden: extensión inequívoca > `db_name` único > núcleo Neo Geo por nombre > `systemid` > nombre del núcleo. */
    fun consoleFor(systemId: String?, coreFile: String, contentPath: String, dbName: String?): Console? {
        contentExt(contentPath)?.let(::byExt)?.let { return it }
        dbName?.let(::byDbName)?.let { return it }
        val byName = byCoreName(coreFile)
        if (byName == Console.NeoGeo) return byName
        return systemId?.let(::bySystemId) ?: byName
    }
}
