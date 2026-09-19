package dev.pepotech.pepomote.control

import org.junit.Assert.*
import org.junit.Test

class UpdatePackagePolicyTest {
    private val installed = UpdatePackageMetadata("dev.pepotech.pepomote", "1.10.0", 22, setOf("signer"))
    private val candidate = installed.copy(versionName = "1.11.0", versionCode = 23)
    @Test fun acceptsOnlyExactPackageVersionAndInstalledSigners() {
        assertNull(UpdatePackagePolicy.rejection(installed, candidate, "1.11.0"))
        assertEquals("package", UpdatePackagePolicy.rejection(installed, candidate.copy(packageName="evil"), "1.11.0"))
        assertEquals("signature", UpdatePackagePolicy.rejection(installed, candidate.copy(signers=setOf("other")), "1.11.0"))
        assertEquals("signature", UpdatePackagePolicy.rejection(installed, candidate.copy(signers=emptySet()), "1.11.0"))
        assertEquals("version", UpdatePackagePolicy.rejection(installed, candidate.copy(versionCode=22), "1.11.0"))
        assertEquals("version", UpdatePackagePolicy.rejection(installed, candidate.copy(versionName="1.12.0"), "1.11.0"))
        assertEquals("version", UpdatePackagePolicy.rejection(installed, candidate.copy(versionName="1.11.0-beta"), "1.11.0"))
    }
    @Test fun retryIsBoundedAndRecoversClockRollback() {
        assertTrue(UpdateSchedule.due(true, 0, 0, 1000))
        assertFalse(UpdateSchedule.due(false, 0, 0, 1000))
        assertFalse(UpdateSchedule.due(true, 0, 1000, 2000))
        assertTrue(UpdateSchedule.due(true, 0, 1000, 301000))
        assertFalse(UpdateSchedule.due(true, 1000, 1000, 301000))
        assertTrue(UpdateSchedule.due(true, 1000, 1000, 3601000))
        assertTrue(UpdateSchedule.due(true, 9000, 9000, 8000))
    }
    @Test fun offerWaitsForSafeForegroundAndOnlyOncePerVersion() {
        assertTrue(UpdateSchedule.offer(true, true, true, "1.11.0", ""))
        assertFalse(UpdateSchedule.offer(false, true, true, "1.11.0", ""))
        assertFalse(UpdateSchedule.offer(true, false, true, "1.11.0", ""))
        assertFalse(UpdateSchedule.offer(true, true, false, "1.11.0", ""))
        assertFalse(UpdateSchedule.offer(true, true, true, "1.11.0", "1.11.0"))
    }
}
