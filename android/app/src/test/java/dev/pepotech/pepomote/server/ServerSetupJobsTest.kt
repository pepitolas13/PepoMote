package dev.pepotech.pepomote.server

import android.content.Context
import android.content.Intent
import android.content.pm.ProviderInfo
import android.net.Uri
import android.provider.DocumentsContract
import dev.pepotech.pepomote.server.setup.EmulatorSetup
import dev.pepotech.pepomote.server.setup.EmulatorTarget
import dev.pepotech.pepomote.server.setup.SetupErrorReason
import dev.pepotech.pepomote.server.setup.SetupException
import dev.pepotech.pepomote.server.setup.SetupResult
import dev.pepotech.pepomote.server.setup.SetupPlayer
import dev.pepotech.pepomote.server.setup.TestEmulatorDocuments
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
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class ServerSetupJobsTest {
    private val context: Context get() = RuntimeEnvironment.getApplication()
    private val packageName = "dev.eden.eden_emulator"

    @Before fun resetVisibleState() {
        ServerSetupJobs.refresh(context)
        ServerSetupJobs.dismissError()
    }

    private fun await(condition: () -> Boolean) {
        val deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(4)
        while (!condition() && System.nanoTime() < deadline) Thread.sleep(10)
        assertTrue("Setup finished", condition())
    }

    @Test fun slowTransactionRetainsRestartBeforeWritingAndWithoutAnyScreenObserver() {
        val (documents, tree) = registerEmulator()
        documents.put("config/config.ini", "[Controls]\n")
        context.contentResolver.takePersistableUriPermission(tree, Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION)
        val entered = CountDownLatch(1)
        val finish = CountDownLatch(1)
        documents.beforeWrite = {
            entered.countDown()
            check(finish.await(4, TimeUnit.SECONDS))
        }
        ServerSetupJobs.prepare(context, EmulatorTarget.Eden, tree, listOf(SetupPlayer(0)))
        try {
            assertTrue(entered.await(4, TimeUnit.SECONDS))
            assertTrue(ServerSetupJobs.flow.value.busy)
            assertEquals(packageName, context.getSharedPreferences("server_setup_pending", Context.MODE_PRIVATE)
                .getString(EmulatorTarget.Eden.name, null))
            // No Compose scope or observer owns this operation; opening another screen cannot cancel it.
            ServerSetupJobs.refresh(context)
            assertEquals(packageName, ServerSetupJobs.flow.value.restart[EmulatorTarget.Eden])
        } finally { finish.countDown() }
        await { !ServerSetupJobs.flow.value.busy }
        assertNull(ServerSetupJobs.flow.value.error)
        assertEquals(packageName, ServerSetupJobs.flow.value.result?.packageName)
        ServerSetupJobs.restartHandled(context, packageName)
        ServerSetupJobs.refresh(context)
        assertFalse(ServerSetupJobs.flow.value.restart.containsKey(EmulatorTarget.Eden))
    }

    @Test fun failureKeepsRecoveryStepAndCanBeDismissedWithoutLosingIt() {
        ServerSetupJobs.run(context, EmulatorTarget.Eden, packageName) {
            throw SetupException("Concurrent emulator edit", reason = SetupErrorReason.RecoveryRequired)
        }
        await { !ServerSetupJobs.flow.value.busy }
        assertEquals(SetupErrorReason.RecoveryRequired,
            (ServerSetupJobs.flow.value.error as SetupException).reason)
        assertEquals(packageName, ServerSetupJobs.flow.value.restart[EmulatorTarget.Eden])
        ServerSetupJobs.dismissError()
        assertNull(ServerSetupJobs.flow.value.error)
        ServerSetupJobs.refresh(context)
        assertEquals(packageName, ServerSetupJobs.flow.value.restart[EmulatorTarget.Eden])
        ServerSetupJobs.restartHandled(context, packageName)
    }

    @Test fun missingPermissionDoesNotStartAJobAndAnExistingGrantSkipsThePicker() {
        val (documents, tree) = registerEmulator()
        documents.put("config/config.ini", "[Controls]\n")
        val previousVersion = ServerSetupJobs.flow.value.version
        assertFalse(ServerSetupJobs.prepare(context, EmulatorTarget.Eden, listOf(SetupPlayer(0))))
        assertEquals(previousVersion, ServerSetupJobs.flow.value.version)
        assertFalse(ServerSetupJobs.flow.value.busy)
        assertTrue(ServerSetupJobs.flow.value.restart.isEmpty())
        context.contentResolver.takePersistableUriPermission(tree, Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION)
        assertTrue(ServerSetupJobs.prepare(context, EmulatorTarget.Eden, listOf(SetupPlayer(0))))
        await { !ServerSetupJobs.flow.value.busy }
        assertNull(ServerSetupJobs.flow.value.error)
        assertTrue(EmulatorSetup.setupStatus(context, EmulatorTarget.Eden).prepared)
        assertEquals(packageName, ServerSetupJobs.flow.value.restart[EmulatorTarget.Eden])
        ServerSetupJobs.restartHandled(context, packageName)
    }

    @Test fun initializationFailureRetainsTheGrantWithoutAPrematureRestartStep() {
        val (documents, tree) = registerEmulator()
        context.contentResolver.takePersistableUriPermission(tree, Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION)
        ServerSetupJobs.prepare(context, EmulatorTarget.Eden, tree, listOf(SetupPlayer(0)))
        await { !ServerSetupJobs.flow.value.busy }
        assertEquals(SetupErrorReason.Initialize, (ServerSetupJobs.flow.value.error as SetupException).reason)
        assertEquals(EmulatorTarget.Eden, ServerSetupJobs.flow.value.target)
        assertTrue(ServerSetupJobs.flow.value.restart.isEmpty())
        ServerSetupJobs.refresh(context)
        assertTrue(ServerSetupJobs.flow.value.restart.isEmpty())
        assertEquals(tree, EmulatorSetup.permissionUri(context, EmulatorTarget.Eden))
        assertEquals(0, documents.writeOpens)
    }

    private fun registerEmulator(): Pair<TestEmulatorDocuments, Uri> {
        val documents = TestEmulatorDocuments(File(context.cacheDir, "jobs-${System.nanoTime()}"))
        val info = ProviderInfo().apply {
            name = TestEmulatorDocuments::class.java.name
            packageName = this@ServerSetupJobsTest.packageName
            authority = "${this@ServerSetupJobsTest.packageName}.user"
            exported = true
            grantUriPermissions = true
            readPermission = "android.permission.MANAGE_DOCUMENTS"
            writePermission = "android.permission.MANAGE_DOCUMENTS"
        }
        shadowOf(context.packageManager).addOrUpdateProvider(info)
        documents.attachInfo(context, info)
        ShadowContentResolver.registerProviderInternal(info.authority, documents)
        return documents to DocumentsContract.buildTreeDocumentUri(info.authority, "root/")
    }
}
