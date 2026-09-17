package dev.pepotech.pepomote.server.retro

import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.ProviderInfo
import android.net.Uri
import android.provider.DocumentsContract
import android.os.Looper
import dev.pepotech.pepomote.server.ServerLifecycleTest
import dev.pepotech.pepomote.server.ServerState
import dev.pepotech.pepomote.server.setup.TestEmulatorDocuments
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Robolectric
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.shadows.ShadowContentResolver
import java.io.File

/** La carpeta RetroArch por SAF (solo lectura) y la detección de RetroArch instalado. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class RetroFolderTest {
    private lateinit var context: Context
    private lateinit var provider: TestEmulatorDocuments
    private val authority = "com.android.externalstorage.documents"
    private val tree: Uri get() = DocumentsContract.buildTreeDocumentUri(authority, "root/")

    private val caveStory = """{"version":"1.5","items":[{"path":"/storage/emulated/0/RetroArch/roms/cave_story_v0.7.0.zip","label":"","core_path":"/data/user/0/com.retroarch.aarch64/cores/genesis_plus_gx_libretro_android.so","core_name":"Sega - MS/GG/MD/CD (Genesis Plus GX)","db_name":"Sega - Mega Drive - Genesis.lpl|Sega - Master System - Mark III.lpl"}]}"""

    @Before fun registerProvider() {
        context = RuntimeEnvironment.getApplication()
        context.getSharedPreferences("retroarch_folder", Context.MODE_PRIVATE).edit().clear().commit()
        context.getSharedPreferences("retroarch_setup", Context.MODE_PRIVATE).edit().clear().commit()
        provider = TestEmulatorDocuments(File(context.cacheDir, "retroarch-${System.nanoTime()}"))
        val info = ProviderInfo().apply {
            name = TestEmulatorDocuments::class.java.name
            packageName = "com.android.externalstorage"
            authority = this@RetroFolderTest.authority
            exported = true
            grantUriPermissions = true
            readPermission = "android.permission.MANAGE_DOCUMENTS"
            writePermission = "android.permission.MANAGE_DOCUMENTS"
        }
        shadowOf(context.packageManager).addOrUpdateProvider(info)
        provider.attachInfo(context, info)
        ShadowContentResolver.registerProviderInternal(authority, provider)
    }

    @Test fun theSavedFolderIsReadOnlyAndTheHistoryIsFoundUnderPlaylistsBuiltin() {
        assertNull(RetroFolder.granted(context))
        provider.put("playlists/builtin/content_history.lpl", caveStory)
        provider.put("saves/keep.srm", "DO NOT TOUCH")
        assertTrue(RetroFolder.save(context, tree))
        assertEquals(tree, RetroFolder.granted(context))
        val kept = context.contentResolver.persistedUriPermissions.single()
        assertTrue(kept.isReadPermission)
        assertFalse("solo lectura", kept.isWritePermission)
        val source = RetroFolder.source(context.contentResolver, tree)
        val document = source.locate()
        assertNotNull(document)
        assertEquals(caveStory, document!!.read())
        val watch = RetroGameWatch(source)
        assertTrue(watch.poll(false))
        assertEquals(Console.Md, watch.game!!.console)
        assertEquals("cave_story_v0.7.0", watch.game!!.title)
        assertEquals(0, provider.writeOpens)
        // Otro juego (tamaño distinto): se relee
        provider.put("playlists/builtin/content_history.lpl", """{"items":[{"path":"/x/Zelda (USA).nes","label":"","core_path":"/x/fceumm_libretro_android.so","core_name":"Nintendo - NES / Famicom (FCEUmm)","db_name":""}]}""")
        assertTrue(watch.poll(false))
        assertEquals(Console.Nes, watch.game!!.console)
        assertEquals("Zelda", watch.game!!.title)
        assertEquals(mapOf("playlists/builtin/content_history.lpl" to provider.text("playlists/builtin/content_history.lpl"), "saves/keep.srm" to "DO NOT TOUCH"), provider.allFiles())
        RetroFolder.forget(context)
        assertNull(RetroFolder.granted(context))
        assertTrue(context.contentResolver.persistedUriPermissions.isEmpty())
    }

    @Test fun olderLayoutsAndMissingHistoryAreHandled() {
        provider.put("content_history.lpl", caveStory)
        assertNotNull("la raíz de instalaciones antiguas también vale", RetroFolder.source(context.contentResolver, tree).locate())
        provider.deleteDocument("root/content_history.lpl")
        provider.put("playlists/other.lpl", "{}")
        assertNull("sin historial no hay documento", RetroFolder.source(context.contentResolver, tree).locate())
        val watch = RetroGameWatch(RetroFolder.source(context.contentResolver, tree))
        assertFalse(watch.poll(false))
        assertFalse(watch.located)
        assertNull(watch.game)
    }

    @Test fun pickerOpensAtTheRetroArchFolderAndRejectsNonTreeUris() {
        val intent = RetroFolder.pickerIntent()
        assertEquals(Intent.ACTION_OPEN_DOCUMENT_TREE, intent.action)
        assertEquals(DocumentsContract.buildDocumentUri(authority, "primary:RetroArch"), intent.getParcelableExtra<Uri>(DocumentsContract.EXTRA_INITIAL_URI))
        assertTrue(intent.flags and Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION != 0)
        assertFalse(RetroFolder.save(context, Uri.parse("file:///sdcard/RetroArch")))
        assertFalse(RetroFolder.save(context, DocumentsContract.buildDocumentUri(authority, "root/")))
        assertNull(RetroFolder.granted(context))
    }

    @Test fun cambiarACarpetaSinHistorialBorraElJuegoDelServidor() {
        provider.put("playlists/builtin/content_history.lpl", caveStory)
        provider.put("vacio/.keep", "")
        assertTrue(RetroFolder.save(context, tree))
        val service = Robolectric.buildService(ServerLifecycleTest.IsolatedService::class.java).create()
        try {
            service.get().onStartCommand(Intent(), 0, 1)
            FakeRetroArch.await(5000, "el servicio anuncia el juego de la primera carpeta") {
                shadowOf(Looper.getMainLooper()).idle()
                ServerState.flow.value.receiver?.retro?.game?.console == Console.Md
            }
            assertTrue(RetroFolder.save(context, DocumentsContract.buildTreeDocumentUri(authority, "root/vacio")))
            FakeRetroArch.await(5000, "la carpeta nueva no puede conservar el juego de la anterior") {
                shadowOf(Looper.getMainLooper()).idle()
                val state = ServerState.flow.value
                state.retroFolder?.located == false && state.receiver?.retro?.game == null
            }
        } finally {
            service.destroy()
            RetroFolder.forget(context)
        }
    }

    @Test fun installedRetroArchIsFoundByItsLauncherAndVerificationIsRemembered() {
        assertNull(RetroArchApp.installedPackage(context))
        assertNull(RetroArchApp.launchIntent(context))
        val component = ComponentName("com.retroarch.aarch64", "com.retroarch.browser.mainmenu.MainMenuActivity")
        shadowOf(context.packageManager).addActivityIfNotPresent(component)
        shadowOf(context.packageManager).addIntentFilterForActivity(component, IntentFilter(Intent.ACTION_MAIN).apply { addCategory(Intent.CATEGORY_LAUNCHER) })
        assertEquals("com.retroarch.aarch64", RetroArchApp.installedPackage(context))
        assertEquals(component.packageName, RetroArchApp.launchIntent(context)!!.component!!.packageName)
        assertNull(RetroArchApp.verifiedVersion(context))
        RetroArchApp.markVerified(context, "1.22.2")
        assertEquals("1.22.2", RetroArchApp.verifiedVersion(context))
        RetroArchApp.markVerified(context, "")
        assertEquals("?", RetroArchApp.verifiedVersion(context))
        RetroArchApp.forgetVerified(context)
        assertNull(RetroArchApp.verifiedVersion(context))
    }
}
