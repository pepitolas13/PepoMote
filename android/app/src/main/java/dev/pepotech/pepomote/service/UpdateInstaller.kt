package dev.pepotech.pepomote.service

import android.content.Context
import android.content.Intent
import android.content.pm.PackageInfo
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.provider.Settings
import androidx.core.content.FileProvider
import dev.pepotech.pepomote.control.UpdateManifest
import dev.pepotech.pepomote.control.UpdatePackageMetadata
import dev.pepotech.pepomote.control.UpdatePackagePolicy
import dev.pepotech.pepomote.net.UpdatePayload
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.withContext
import java.io.File
import java.io.OutputStream
import java.security.MessageDigest

/** Exposes cache/updates exclusively. No broad external-storage or app-data access. */
class UpdateFileProvider : FileProvider()

object UpdateInstaller {
    fun directory(context: Context): File = File(context.cacheDir, "updates").apply {
        check(isDirectory || mkdirs()) { "Cannot create update cache" }
    }
    /** Lookup and best-effort cleanup must never attempt to allocate a directory. */
    fun file(context: Context, release: UpdateManifest) = File(File(context.cacheDir, "updates"), "PepoMote-${release.version}.apk")
    fun canInstall(context: Context) = context.packageManager.canRequestPackageInstalls()

    fun permissionIntent(context: Context) = Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES,
        Uri.parse("package:${context.packageName}"))

    fun installIntent(context: Context, file: File): Intent = Intent(Intent.ACTION_VIEW).apply {
        setDataAndType(FileProvider.getUriForFile(context, "${context.packageName}.update-files", file), "application/vnd.android.package-archive")
        addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
    }

    @Suppress("DEPRECATION")
    suspend fun verify(context: Context, release: UpdateManifest): File = withContext(Dispatchers.IO) {
        val root = directory(context).canonicalFile
        val apk = file(context, release).canonicalFile
        require(apk.parentFile == root && apk.isFile && apk.length() == release.asset.size) { "Invalid private APK" }
        val coroutine = currentCoroutineContext()
        apk.inputStream().buffered().use { input ->
            UpdatePayload.copyVerified(input, object : OutputStream() {
                override fun write(b: Int) {}
                override fun write(b: ByteArray, off: Int, len: Int) {}
            }, release.asset, { coroutine.ensureActive() }, {})
        }
        val flags = if (Build.VERSION.SDK_INT >= 28) PackageManager.GET_SIGNING_CERTIFICATES else PackageManager.GET_SIGNATURES
        val manager = context.packageManager
        val candidate = requireNotNull(manager.getPackageArchiveInfo(apk.path, flags)) { "Unreadable APK" }
        val installed = manager.getPackageInfo(context.packageName, flags)
        val rejection = UpdatePackagePolicy.rejection(metadata(installed), metadata(candidate), release.version.toString())
        require(rejection == null) { "APK $rejection does not match this installation" }
        apk
    }

    @Suppress("DEPRECATION")
    private fun metadata(info: PackageInfo): UpdatePackageMetadata {
        val signers = if (Build.VERSION.SDK_INT >= 28) info.signingInfo?.apkContentsSigners else info.signatures
        return UpdatePackageMetadata(info.packageName, info.versionName,
            if (Build.VERSION.SDK_INT >= 28) info.longVersionCode else info.versionCode.toLong(),
            signers?.map { UpdatePayload.hex(MessageDigest.getInstance("SHA-256").digest(it.toByteArray())) }?.toSet() ?: emptySet())
    }
}
