package dev.pepotech.pepomote.server

import android.content.Context
import dev.pepotech.pepomote.R
import dev.pepotech.pepomote.server.setup.SetupErrorReason
import dev.pepotech.pepomote.server.setup.SetupException

internal fun setupErrorText(context: Context, error: Exception): String {
    val reason = (error as? SetupException)?.reason ?: if (error is SecurityException)
        SetupErrorReason.Permission else SetupErrorReason.Access
    return context.getString(when (reason) {
        SetupErrorReason.Access -> R.string.setup_error_access
        SetupErrorReason.Selection -> R.string.setup_error_selection
        SetupErrorReason.Permission -> R.string.setup_error_permission
        SetupErrorReason.Initialize -> R.string.setup_error_initialize
        SetupErrorReason.Conflict -> R.string.setup_error_conflict
        SetupErrorReason.Backup -> R.string.setup_error_backup
        SetupErrorReason.RolledBack -> R.string.setup_error_rolled_back
        SetupErrorReason.RecoveryRequired -> R.string.setup_error_recovery
        SetupErrorReason.NoBackup -> R.string.setup_error_no_backup
    })
}
