package dev.pepotech.pepomote.control

/** Read from PackageManager; never trust the manifest for package identity. */
data class UpdatePackageMetadata(val packageName: String, val versionName: String?, val versionCode: Long, val signers: Set<String>)

object UpdatePackagePolicy {
    fun rejection(installed: UpdatePackageMetadata, candidate: UpdatePackageMetadata, expectedVersion: String): String? {
        if (candidate.packageName != installed.packageName) return "package"
        if (candidate.signers.isEmpty() || candidate.signers != installed.signers) return "signature"
        if (candidate.versionName != expectedVersion || candidate.versionCode <= installed.versionCode) return "version"
        return null
    }
}

object UpdateSchedule {
    const val RETRY_MS = 5L * 60 * 1000
    fun due(enabled: Boolean, successMs: Long, attemptMs: Long, nowMs: Long): Boolean =
        UpdateCheck.due(enabled, successMs, nowMs) &&
            (attemptMs == 0L || nowMs < attemptMs || nowMs - attemptMs >= RETRY_MS)

    fun offer(enabled: Boolean, foreground: Boolean, safeScreen: Boolean, version: String, offered: String): Boolean =
        enabled && foreground && safeScreen && version != offered
}
