package dev.pepotech.pepomote.server.setup

import android.content.Context
import android.content.ContextWrapper
import android.content.Intent
import android.content.pm.ProviderInfo
import android.database.Cursor
import android.database.MatrixCursor
import android.net.Uri
import android.os.CancellationSignal
import android.os.ParcelFileDescriptor
import android.provider.DocumentsContract
import android.provider.DocumentsProvider
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.shadows.ShadowContentResolver
import java.io.File
import java.io.FileNotFoundException

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class EmulatorSetupAndroidTest {
    private lateinit var context: Context
    private lateinit var provider: TestEmulatorDocuments
    private val packageName = "dev.eden.eden_emulator"
    private val authority = "$packageName.user"
    private val tree: Uri get() = DocumentsContract.buildTreeDocumentUri(authority, "root/")

    @Before fun registerProviderAndGrantTree() {
        context = RuntimeEnvironment.getApplication()
        provider = TestEmulatorDocuments(File(context.cacheDir, "provider-${System.nanoTime()}"))
        val info = ProviderInfo().apply {
            name = TestEmulatorDocuments::class.java.name
            packageName = this@EmulatorSetupAndroidTest.packageName
            authority = this@EmulatorSetupAndroidTest.authority
            exported = true
            grantUriPermissions = true
            readPermission = "android.permission.MANAGE_DOCUMENTS"
            writePermission = "android.permission.MANAGE_DOCUMENTS"
        }
        shadowOf(context.packageManager).addOrUpdateProvider(info)
        provider.attachInfo(context, info)
        ShadowContentResolver.registerProviderInternal(authority, provider)
        context.contentResolver.takePersistableUriPermission(tree, Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION)
    }

    @Test fun nativeSafPreparationAndRestoreKeepSaveDataAndExactOriginalConfig() {
        val original = "\uFEFF; custom settings\r\n[Controls]\r\nplayer_0_type = 1\r\n[Renderer]\r\nbackend=2\r\n"
        provider.put("config/config.ini", original)
        provider.put("nand/user/save.dat", "DO NOT TOUCH")
        val result = EmulatorSetup.prepare(context, EmulatorTarget.Eden, tree)
        assertEquals(packageName, result.packageName)
        assertTrue(result.needsRestart)
        assertEquals(listOf("config.ini"), result.changedFiles)
        assertTrue(EmulatorSetup.hasBackup(context, EmulatorTarget.Eden))
        assertEquals("0", IniDocument.parse(provider.text("config/config.ini")).value(IniKey("Controls", "player_0_type")))
        EmulatorSetup.restore(context, EmulatorTarget.Eden)
        assertEquals(original, provider.text("config/config.ini"))
        assertEquals("DO NOT TOUCH", provider.text("nand/user/save.dat"))
        assertFalse(EmulatorSetup.hasBackup(context, EmulatorTarget.Eden))
    }

    @Test fun privateJournalCanAtomicallyReplaceAndReadBackAnEarlierBackup() {
        val backups = AndroidBackupStore(context, EmulatorTarget.Eden)
        backups.save(SetupJournal(emptyList()))
        val next = SetupJournal(listOf(ManagedFile("config.ini", null, "[Controls]\n".toByteArray(), emptyList())))
        backups.save(next)
        assertEquals("config.ini", backups.load()!!.files.single().path)
    }

    @Test fun outputCloseFailureCannotClaimOwnershipOfANewerEmulatorWrite() {
        provider.put("config/config.ini", "[Controls]\nplayer_0_type=1\n")
        val newer = "[Controls]\nplayer_0_type=2\n[User]\nnote=do not overwrite\n"
        val document = DocumentsContract.buildDocumentUriUsingTree(tree, "root/config/config.ini")
        var opens = 0
        shadowOf(context.contentResolver).registerOutputStreamSupplier(document) {
            val failOnClose = ++opens == 1
            object : java.io.OutputStream() {
                private val bytes = java.io.ByteArrayOutputStream()
                override fun write(value: Int) { bytes.write(value) }
                override fun write(data: ByteArray, offset: Int, length: Int) { bytes.write(data, offset, length) }
                override fun flush() { provider.put("config/config.ini", bytes.toString(Charsets.UTF_8.name())) }
                override fun close() {
                    flush()
                    if (failOnClose) {
                        provider.put("config/config.ini", newer)
                        throw java.io.IOException("Provider closed after emulator saved newer settings")
                    }
                }
            }
        }
        val failure = assertThrows(SetupException::class.java) { EmulatorSetup.prepare(context, EmulatorTarget.Eden, tree) }
        assertEquals(SetupErrorReason.RecoveryRequired, failure.reason)
        assertEquals(newer, provider.text("config/config.ini"))
        assertEquals(1, opens)
        assertTrue(EmulatorSetup.hasBackup(context, EmulatorTarget.Eden))
    }

    @Test fun configSubtreeGrantWorksWithoutAccessToOtherUserFolders() {
        provider.put("config/config.ini", "[Controls]\n")
        val configTree = DocumentsContract.buildTreeDocumentUri(authority, "root/config")
        context.contentResolver.takePersistableUriPermission(configTree, Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION)
        EmulatorSetup.prepare(context, EmulatorTarget.Eden, configTree)
        assertTrue(provider.requestedIds.isNotEmpty())
        assertEquals("true", IniDocument.parse(provider.text("config/config.ini")).value(IniKey("Controls", "player_0_connected")))
        assertTrue(provider.requestedIds.all { it == "root/config" || it.startsWith("root/config/") })
    }

    @Test fun wrongProviderOwnerAndDirectoryDisguisedAsIniAreRejectedWithoutWriting() {
        provider.put("config/config.ini/inside", "untouched")
        assertThrows(SetupException::class.java) { EmulatorSetup.prepare(context, EmulatorTarget.Eden, tree) }
        assertEquals(0, provider.writeOpens)
        val info = context.packageManager.resolveContentProvider(authority, 0)!!
        info.packageName = "com.fake.Eden"
        shadowOf(context.packageManager).addOrUpdateProvider(info)
        assertThrows(SetupException::class.java) { EmulatorSetup.prepare(context, EmulatorTarget.Eden, tree) }
        assertEquals(0, provider.writeOpens)
    }

    @Test fun returnedChildOutsideSelectedConfigCannotBeWritten() {
        provider.put("config/config.ini", "[Controls]\n")
        provider.forgeConfigChildId = true
        assertThrows(SetupException::class.java) { EmulatorSetup.prepare(context, EmulatorTarget.Eden, tree) }
        assertEquals(0, provider.writeOpens)
    }

    @Test fun exactInitialUriHintsAndPackageDiscoveryUseOfficialAuthorities() {
        assertEquals(packageName, EmulatorSetup.installedPackage(context, EmulatorTarget.Eden))
        // These providers have no findDocumentPath: a document hint falls back to the last folder.
        assertEquals(DocumentsContract.buildRootUri(authority, "root"), EmulatorSetup.initialUri(context, EmulatorTarget.Eden))
        assertEquals(DocumentsContract.buildRootUri(authority, "root"), EmulatorSetup.initialUri(EmulatorTarget.Eden))
        assertEquals(DocumentsContract.buildRootUri("org.dolphinemu.dolphinemu.user", "root"), EmulatorSetup.initialUri(EmulatorTarget.Dolphin))
    }

    @Test fun pickerHasNoFallbackToAnUninstalledEmulator() {
        assertNull(EmulatorSetup.initialUri(context, EmulatorTarget.Dolphin))
    }

    @Test @Config(sdk = [28]) fun olderAndroidKeepsItsSupportedDocumentHint() {
        assertEquals(DocumentsContract.buildDocumentUri(authority, "root/"), EmulatorSetup.initialUri(context, EmulatorTarget.Eden))
    }

    @Test fun unavailableOrUnsafeProvidersAreNotOfferedForAutomaticSetup() {
        val info = context.packageManager.resolveContentProvider(authority, 0)!!
        for (setting in listOf("private", "no-grants", "disabled")) {
            info.exported = setting != "private"
            info.grantUriPermissions = setting != "no-grants"
            info.enabled = setting != "disabled"
            shadowOf(context.packageManager).addOrUpdateProvider(info)
            assertNull(setting, EmulatorSetup.installedPackage(context, EmulatorTarget.Eden))
            assertNull(setting, EmulatorSetup.initialUri(context, EmulatorTarget.Eden))
        }
        assertEquals(0, provider.writeOpens)
    }

    @Test fun reusablePermissionRequiresAnExactSupportedTreeWithReadAndWriteAccess() {
        assertEquals(tree, EmulatorSetup.permissionUri(context, EmulatorTarget.Eden))
        assertTrue("Looking up a permission must not start the emulator provider", provider.requestedIds.isEmpty())
        context.contentResolver.releasePersistableUriPermission(tree, Intent.FLAG_GRANT_WRITE_URI_PERMISSION)
        assertNull(EmulatorSetup.permissionUri(context, EmulatorTarget.Eden))
        val savesTree = DocumentsContract.buildTreeDocumentUri(authority, "root/nand")
        context.contentResolver.takePersistableUriPermission(savesTree, Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION)
        assertNull(EmulatorSetup.permissionUri(context, EmulatorTarget.Eden))
        val configTree = DocumentsContract.buildTreeDocumentUri(authority, "root/config")
        context.contentResolver.takePersistableUriPermission(configTree, Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION)
        assertEquals(configTree, EmulatorSetup.permissionUri(context, EmulatorTarget.Eden))
        assertTrue(provider.requestedIds.isEmpty())
    }

    @Test fun preparedStatusTracksOnlyCompletedSetupAndPlayersWithoutReopeningTheEmulator() {
        provider.put("config/config.ini", "[Controls]\n")
        assertFalse(EmulatorSetup.setupStatus(context, EmulatorTarget.Eden).prepared)
        val players = listOf(SetupPlayer(2), SetupPlayer(0, true))
        EmulatorSetup.prepare(context, EmulatorTarget.Eden, tree, players)
        provider.requestedIds.clear()
        val status = EmulatorSetup.setupStatus(context, EmulatorTarget.Eden)
        assertEquals(packageName, status.packageName)
        assertEquals(tree, status.permissionUri)
        assertTrue(status.hasBackup)
        assertTrue(status.prepared)
        assertEquals(players.sortedBy { it.slot }, status.configuredPlayers)
        assertFalse(status.recoveryRequired)
        assertTrue("Status must not restart a force-stopped emulator", provider.requestedIds.isEmpty())
        EmulatorSetup.prepare(context, EmulatorTarget.Eden, tree, listOf(SetupPlayer(1)))
        assertEquals(listOf(SetupPlayer(1)), EmulatorSetup.setupStatus(context, EmulatorTarget.Eden).configuredPlayers)
        EmulatorSetup.restore(context, EmulatorTarget.Eden)
        val restored = EmulatorSetup.setupStatus(context, EmulatorTarget.Eden)
        assertFalse(restored.prepared)
        assertFalse(restored.hasBackup)
        assertTrue(restored.configuredPlayers.isEmpty())
        assertEquals(tree, restored.permissionUri)
        assertEquals("[Controls]\n", provider.text("config/config.ini"))
    }

    @Test fun interruptedTransactionOrOldBackupWithoutCompletionNeverReportsPrepared() {
        provider.put("config/config.ini", "[Controls]\n")
        EmulatorSetup.prepare(context, EmulatorTarget.Eden, tree)
        val backups = AndroidBackupStore(context, EmulatorTarget.Eden)
        val journal = backups.load()!!
        backups.save(journal.copy(pending = PendingTransaction(emptyList(), journal.files)))
        val pending = EmulatorSetup.setupStatus(context, EmulatorTarget.Eden)
        assertTrue(pending.hasBackup)
        assertTrue(pending.recoveryRequired)
        assertFalse(pending.prepared)
        context.getSharedPreferences("emulator_setup", Context.MODE_PRIVATE).edit().clear().commit()
        backups.save(journal)
        assertFalse(EmulatorSetup.setupStatus(context, EmulatorTarget.Eden).prepared)
        assertTrue(EmulatorSetup.setupStatus(context, EmulatorTarget.Eden).hasBackup)
    }

    @Test fun initializationFailureKeepsTheSelectedVariantAndPermissionForTheNextTap() {
        val legacyPackage = "dev.legacy.eden_emulator"
        val legacy = registerVariant(legacyPackage)
        val legacyTree = DocumentsContract.buildTreeDocumentUri("$legacyPackage.user", "root/")
        context.contentResolver.takePersistableUriPermission(legacyTree, Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION)
        val failure = assertThrows(SetupException::class.java) { EmulatorSetup.prepare(context, EmulatorTarget.Eden, legacyTree) }
        assertEquals(SetupErrorReason.Initialize, failure.reason)
        assertEquals(legacyPackage, EmulatorSetup.installedPackage(context, EmulatorTarget.Eden))
        assertEquals(legacyTree, EmulatorSetup.permissionUri(context, EmulatorTarget.Eden))
        assertFalse(EmulatorSetup.setupStatus(context, EmulatorTarget.Eden).prepared)
        legacy.put("config/config.ini", "[Controls]\n")
        EmulatorSetup.prepare(context, EmulatorTarget.Eden, EmulatorSetup.permissionUri(context, EmulatorTarget.Eden)!!)
        assertTrue(EmulatorSetup.setupStatus(context, EmulatorTarget.Eden).prepared)
        assertEquals(0, provider.writeOpens)
    }

    @Test fun observedMissingConfigurationInvalidatesEarlierCompletionWithoutDiscardingBackup() {
        provider.put("config/config.ini", "[Controls]\n")
        EmulatorSetup.prepare(context, EmulatorTarget.Eden, tree)
        assertTrue(EmulatorSetup.setupStatus(context, EmulatorTarget.Eden).prepared)
        provider.deleteDocument("root/config/config.ini")
        val failure = assertThrows(SetupException::class.java) { EmulatorSetup.prepare(context, EmulatorTarget.Eden, tree) }
        assertEquals(SetupErrorReason.Initialize, failure.reason)
        val status = EmulatorSetup.setupStatus(context, EmulatorTarget.Eden)
        assertFalse(status.prepared)
        assertTrue(status.hasBackup)
        assertEquals(tree, status.permissionUri)
    }

    @Test fun missingBackedUpVariantCannotBeReplacedByAnotherInstalledEden() {
        provider.put("config/config.ini", "[Controls]\n")
        EmulatorSetup.prepare(context, EmulatorTarget.Eden, tree)
        val oldConfig = provider.text("config/config.ini")
        val info = context.packageManager.resolveContentProvider(authority, 0)!!
        info.enabled = false
        shadowOf(context.packageManager).addOrUpdateProvider(info)
        val legacyPackage = "dev.legacy.eden_emulator"
        val legacy = registerVariant(legacyPackage)
        legacy.put("config/config.ini", "[Controls]\n")
        val legacyTree = DocumentsContract.buildTreeDocumentUri("$legacyPackage.user", "root/")
        context.contentResolver.takePersistableUriPermission(legacyTree, Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION)
        val status = EmulatorSetup.setupStatus(context, EmulatorTarget.Eden)
        assertNull(status.packageName)
        assertNull(status.permissionUri)
        assertTrue(status.hasBackup)
        assertEquals(packageName, status.backupPackageName)
        assertFalse(status.prepared)
        val failure = assertThrows(SetupException::class.java) { EmulatorSetup.prepare(context, EmulatorTarget.Eden, legacyTree) }
        assertEquals(SetupErrorReason.Conflict, failure.reason)
        assertEquals(oldConfig, provider.text("config/config.ini"))
        assertEquals(0, legacy.writeOpens)
    }

    private fun registerVariant(variant: String): TestEmulatorDocuments {
        val documents = TestEmulatorDocuments(File(context.cacheDir, "variant-${System.nanoTime()}"))
        val info = ProviderInfo().apply {
            name = TestEmulatorDocuments::class.java.name
            packageName = variant
            authority = "$variant.user"
            exported = true
            grantUriPermissions = true
            readPermission = "android.permission.MANAGE_DOCUMENTS"
            writePermission = "android.permission.MANAGE_DOCUMENTS"
        }
        shadowOf(context.packageManager).addOrUpdateProvider(info)
        documents.attachInfo(context, info)
        ShadowContentResolver.registerProviderInternal("$variant.user", documents)
        return documents
    }

    @Test fun uninitializedEdenAndReadOnlyGrantDoNotCreateConfiguration() {
        assertThrows(SetupException::class.java) { EmulatorSetup.prepare(context, EmulatorTarget.Eden, tree) }
        assertEquals(0, provider.writeOpens)
        assertFalse(EmulatorSetup.hasBackup(context, EmulatorTarget.Eden))
        provider.put("config/config.ini", "[Controls]\n")
        context.contentResolver.releasePersistableUriPermission(tree, Intent.FLAG_GRANT_WRITE_URI_PERMISSION)
        val denied = object : ContextWrapper(context) {
            override fun getApplicationContext(): Context = this
            override fun checkUriPermission(uri: Uri, pid: Int, uid: Int, modeFlags: Int) = android.content.pm.PackageManager.PERMISSION_DENIED
        }
        val failure = assertThrows(SetupException::class.java) { EmulatorSetup.prepare(denied, EmulatorTarget.Eden, tree) }
        assertEquals(SetupErrorReason.Permission, failure.reason)
        assertEquals(0, provider.writeOpens)
    }

    @Test fun dolphinCreatesBothProfileVariantsAndRestoresOnlyItsFiles() {
        val dolphinPackage = "org.dolphinemu.dolphinemu"
        val dolphinAuthority = "$dolphinPackage.user"
        provider = TestEmulatorDocuments(File(context.cacheDir, "dolphin-${System.nanoTime()}"))
        val info = ProviderInfo().apply {
            name = TestEmulatorDocuments::class.java.name
            packageName = dolphinPackage
            authority = dolphinAuthority
            exported = true
            grantUriPermissions = true
            readPermission = "android.permission.MANAGE_DOCUMENTS"
            writePermission = "android.permission.MANAGE_DOCUMENTS"
        }
        shadowOf(context.packageManager).addOrUpdateProvider(info)
        provider.attachInfo(context, info)
        ShadowContentResolver.registerProviderInternal(dolphinAuthority, provider)
        val dolphinTree = DocumentsContract.buildTreeDocumentUri(dolphinAuthority, "root/")
        context.contentResolver.takePersistableUriPermission(dolphinTree, Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION)
        val original = "[Wiimote1]\nSource = 0\n[Wiimote2]\nSource = 2\n"
        provider.put("Config/WiimoteNew.ini", original)
        provider.put("Wii/title/save.bin", "KEEP SAVE")
        val result = EmulatorSetup.prepare(context, EmulatorTarget.Dolphin, dolphinTree, listOf(SetupPlayer(0, true)))
        assertEquals(8, result.profileNames.size)
        assertTrue(provider.text("Config/Profiles/Wiimote/PepoMote Android J4 Nunchuk.ini").contains("DSUClient/3/PepoMote"))
        val active = IniDocument.parse(provider.text("Config/WiimoteNew.ini"))
        assertEquals("Nunchuk", active.value(IniKey("Wiimote1", "Extension")))
        assertEquals("2", active.value(IniKey("Wiimote2", "Source")))
        EmulatorSetup.restore(context, EmulatorTarget.Dolphin)
        assertEquals(mapOf("Config/WiimoteNew.ini" to original, "Wii/title/save.bin" to "KEEP SAVE"), provider.allFiles())
    }
}

/** A real DocumentsProvider backed by an isolated test directory, including ParcelFileDescriptor IO. */
class TestEmulatorDocuments(private val directory: File) : DocumentsProvider() {
    var writeOpens = 0
    var beforeWrite: (() -> Unit)? = null
    var forgeConfigChildId = false
    val requestedIds = mutableListOf<String>()

    override fun onCreate(): Boolean { directory.mkdirs(); return true }
    fun put(path: String, text: String) { File(directory, path).apply { parentFile!!.mkdirs(); writeText(text) } }
    fun text(path: String) = File(directory, path).readText()
    fun allFiles() = directory.walkTopDown().filter { it.isFile }.associate { it.relativeTo(directory).invariantSeparatorsPath to it.readText() }
    private fun file(id: String): File {
        requestedIds += id
        if (id != "root" && id != "root/" && !id.startsWith("root/")) throw FileNotFoundException()
        return if (id == "root" || id == "root/") directory else File(directory, id.removePrefix("root/"))
    }
    private fun id(file: File) = "root/" + file.relativeTo(directory).invariantSeparatorsPath
    private fun row(file: File, projection: Array<out String>? = null): MatrixCursor {
        val columns = projection ?: arrayOf(DocumentsContract.Document.COLUMN_DOCUMENT_ID, DocumentsContract.Document.COLUMN_DISPLAY_NAME, DocumentsContract.Document.COLUMN_MIME_TYPE, DocumentsContract.Document.COLUMN_FLAGS, DocumentsContract.Document.COLUMN_SIZE)
        return MatrixCursor(columns).also { cursor ->
            cursor.addRow(columns.map<String, Any?> { column -> when (column) {
                DocumentsContract.Document.COLUMN_DOCUMENT_ID -> if (forgeConfigChildId && file.name == "config.ini") "root/nand/save.dat" else id(file)
                DocumentsContract.Document.COLUMN_DISPLAY_NAME -> file.name
                DocumentsContract.Document.COLUMN_MIME_TYPE -> if (file.isDirectory) DocumentsContract.Document.MIME_TYPE_DIR else "application/octet-stream"
                DocumentsContract.Document.COLUMN_FLAGS -> if (file.isDirectory) DocumentsContract.Document.FLAG_DIR_SUPPORTS_CREATE else DocumentsContract.Document.FLAG_SUPPORTS_WRITE or DocumentsContract.Document.FLAG_SUPPORTS_DELETE
                DocumentsContract.Document.COLUMN_SIZE -> file.length()
                else -> null
            } }.toTypedArray())
        }
    }
    override fun queryRoots(projection: Array<out String>?): Cursor = MatrixCursor(arrayOf(DocumentsContract.Root.COLUMN_ROOT_ID, DocumentsContract.Root.COLUMN_DOCUMENT_ID)).apply { addRow(arrayOf("root", "root/")) }
    override fun queryDocument(documentId: String, projection: Array<out String>?): Cursor = row(file(documentId), projection)
    override fun queryChildDocuments(parentDocumentId: String, projection: Array<out String>?, sortOrder: String?): Cursor {
        val children = file(parentDocumentId).listFiles().orEmpty()
        val columns = projection ?: arrayOf(DocumentsContract.Document.COLUMN_DOCUMENT_ID, DocumentsContract.Document.COLUMN_DISPLAY_NAME, DocumentsContract.Document.COLUMN_MIME_TYPE, DocumentsContract.Document.COLUMN_FLAGS, DocumentsContract.Document.COLUMN_SIZE)
        return MatrixCursor(columns).also { cursor ->
            children.forEach { child -> row(child, columns).use { single ->
                single.moveToFirst()
                cursor.addRow(columns.indices.map { index -> if (single.getType(index) == Cursor.FIELD_TYPE_INTEGER) single.getLong(index) else single.getString(index) }.toTypedArray())
            } }
        }
    }
    override fun openDocument(documentId: String, mode: String, signal: CancellationSignal?): ParcelFileDescriptor {
        if (mode != "r") {
            beforeWrite?.invoke()
            writeOpens++
        }
        return ParcelFileDescriptor.open(file(documentId), ParcelFileDescriptor.parseMode(mode))
    }
    override fun createDocument(parentDocumentId: String, mimeType: String, displayName: String): String {
        val file = File(file(parentDocumentId), displayName)
        if (file.exists()) throw FileNotFoundException("collision")
        if (mimeType == DocumentsContract.Document.MIME_TYPE_DIR) file.mkdir() else file.createNewFile()
        return id(file)
    }
    override fun deleteDocument(documentId: String) { if (!file(documentId).delete()) throw FileNotFoundException() }
    override fun isChildDocument(parentDocumentId: String, documentId: String) = documentId.startsWith(parentDocumentId.trimEnd('/') + "/")
}
