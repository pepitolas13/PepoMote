package dev.pepotech.pepomote.service

import dev.pepotech.pepomote.control.UpdateSchedule
import org.junit.Assert.*
import org.junit.Test

class UpdateLifecycleTest {
    @Test fun closingVerificationRevokesThePendingNativeLaunch() {
        val open = UpdateUiState(phase = UpdatePhase.Verifying, dialog = true)
        assertTrue(open.mayLaunchInstaller(true))
        assertFalse(open.copy(dialog = false).mayLaunchInstaller(true))
        assertFalse(open.mayLaunchInstaller(false))
    }
    @Test fun nativePermissionAndInstallerPreventConcurrentManifestReplacement() {
        for (phase in listOf(UpdatePhase.PermissionRequired, UpdatePhase.InstallerOpened)) {
            val state = UpdateUiState(phase = phase, externalPending = true)
            assertFalse(state.canCheck(checkRunning = false, transferRunning = false))
            val returned = if (phase == UpdatePhase.PermissionRequired) state.permissionResult(false) else state.installerReturned()
            assertTrue(returned.canCheck(checkRunning = false, transferRunning = false))
            assertFalse(returned.autoInstall)
        }
    }
    @Test fun doubleCheckAndDoubleDownloadCannotStartAnotherOperation() {
        val state = UpdateUiState()
        assertFalse(state.canCheck(checkRunning = true, transferRunning = false))
        assertFalse(state.canCheck(checkRunning = false, transferRunning = true))
        assertTrue(state.canCheck(checkRunning = false, transferRunning = false))
    }
    @Test fun permissionResumeIsOneShotAndOnlyOnGrant() {
        val pending = UpdateUiState(phase = UpdatePhase.PermissionRequired, externalPending = true)
        val granted = pending.permissionResult(true)
        assertTrue(granted.autoInstall)
        assertFalse(granted.externalPending)
        assertEquals(UpdatePhase.Ready, granted.phase)
        assertFalse(pending.permissionResult(false).autoInstall)
        assertFalse(granted.installerReturned().autoInstall)
    }
    @Test fun resumeDuringGameplayDefersOfferAndLaterVersionOffersAgain() {
        assertFalse(UpdateSchedule.offer(true, true, false, "1.12.0", "1.11.0"))
        assertTrue(UpdateSchedule.offer(true, true, true, "1.12.0", "1.11.0"))
        assertFalse(UpdateSchedule.offer(true, true, true, "1.12.0", "1.12.0"))
        assertFalse(UpdateSchedule.offer(true, false, true, "1.12.0", "1.11.0"))
    }
}
