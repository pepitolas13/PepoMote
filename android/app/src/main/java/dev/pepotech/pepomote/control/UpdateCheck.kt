package dev.pepotech.pepomote.control

/**
 * Aviso de versión nueva: la lógica pura (sin Android). Privacidad: la única
 * petición que sale de la red local es un HEAD a `releases/latest` de GitHub,
 * que responde con una redirección a la etiqueta de la última versión
 * publicada. Sin cuerpo, sin identificadores, una vez al día, y se apaga en
 * Ajustes. Mismos casos que el receptor (desktop/src/update.rs) y que iOS.
 */
object UpdateCheck {
    const val REPO = "pepitolas13/PepoMote"
    const val LATEST_URL = "https://github.com/$REPO/releases/latest"
    /** Fijo y sin versión: GitHub solo ve «un PepoMote», nada más. */
    const val USER_AGENT = "PepoMote-update-check"
    const val CHECK_EVERY_MS = 24L * 3600 * 1000
    /** La primera consulta espera a que el inicio esté en pantalla. */
    const val FIRST_DELAY_MS = 3000L
    const val TIMEOUT_MS = 5000

    /** `mayor.menor.parche`, comparada numéricamente (1.10 > 1.9). */
    data class Version(val major: Int, val minor: Int, val patch: Int) : Comparable<Version> {
        override fun compareTo(other: Version): Int =
            compareValuesBy(this, other, { it.major }, { it.minor }, { it.patch })

        override fun toString() = "$major.$minor.$patch"
    }

    /** `v1.6.0`, `1.6.0` o `v1.6` (→ 1.6.0). Sufijos (`-beta`) o basura → null. */
    fun parse(s: String?): Version? {
        var t = s?.trim() ?: return null
        if (t.startsWith("v") || t.startsWith("V")) t = t.substring(1)
        if (t.isEmpty()) return null
        val parts = t.split('.')
        if (parts.size !in 2..3) return null
        val nums = ArrayList<Int>(3)
        for (p in parts) {
            if (p.isEmpty() || !p.all { it.isDigit() }) return null
            nums.add(p.toIntOrNull() ?: return null)
        }
        return Version(nums[0], nums[1], nums.getOrElse(2) { 0 })
    }

    /** `…/releases/tag/v1.6.0` → 1.6.0 (también con `?x`, `#x` o `/` detrás). */
    fun versionFromLocation(location: String?): Version? {
        if (location == null) return null
        val i = location.lastIndexOf("/tag/")
        if (i < 0) return null
        val tag = location.substring(i + 5).takeWhile { it != '?' && it != '#' && it != '/' }
        return parse(tag)
    }

    /** Página de la release en GitHub (la que se abre desde el aviso). */
    fun releaseUrl(v: Version) = "https://github.com/$REPO/releases/tag/v$v"

    /**
     * La versión que hay que anunciar: la última publicada si es mayor que la
     * actual y el usuario no la ocultó (una posterior sí se anuncia).
     */
    fun pending(current: Version?, latest: Version?, dismissed: Version?): Version? {
        if (current == null || latest == null) return null
        if (latest <= current) return null
        if (latest == dismissed) return null
        return latest
    }

    /**
     * ¿Toca consultar? Activado y, o nunca se consultó (`lastMs == 0`), o han
     * pasado 24 h desde la última vez (un reloj hacia atrás no dispara nada).
     */
    fun due(enabled: Boolean, lastMs: Long, nowMs: Long): Boolean =
        enabled && (lastMs == 0L || (nowMs - lastMs) >= CHECK_EVERY_MS)
}
