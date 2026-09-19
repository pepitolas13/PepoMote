package dev.pepotech.pepomote.service

import android.content.ContextWrapper
import dev.pepotech.pepomote.control.UpdateAsset
import dev.pepotech.pepomote.control.UpdateCheck
import dev.pepotech.pepomote.control.UpdateManifest
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import java.io.File

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class UpdateInstallerTest {
    @get:Rule val temporary = TemporaryFolder()
    @Test fun inspectingOrCleaningMissingCacheDoesNotCreateDirectoriesOrThrow() {
        val cache = temporary.newFolder("cache")
        val context = object : ContextWrapper(RuntimeEnvironment.getApplication()) { override fun getCacheDir() = cache }
        val release = UpdateManifest(UpdateCheck.Version(1,11,0), "", emptyMap(), UpdateAsset("PepoMote.apk", "", 1, "a".repeat(64)), "")
        val file = UpdateInstaller.file(context, release)
        assertFalse(File(cache, "updates").exists())
        File(cache, "updates").writeText("blocked directory")
        assertFalse(UpdateInstaller.file(context, release).isFile)
        assertFalse(UpdateInstaller.file(context, release).delete())
        assertEquals(File(cache, "updates/PepoMote-1.11.0.apk"), file)
    }
}
