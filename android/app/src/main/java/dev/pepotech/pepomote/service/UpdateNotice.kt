package dev.pepotech.pepomote.service

import android.content.Context
import dev.pepotech.pepomote.control.AppPrefs
import dev.pepotech.pepomote.control.UpdateCheck
import dev.pepotech.pepomote.net.UpdateClient
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow

/**
 * Estado del aviso de versión nueva para la UI: la versión pendiente de
 * anunciar (o null). Lo guardado vive en AppPrefs; aquí solo se deriva.
 */
object UpdateNotice {
    private val _pending = MutableStateFlow<UpdateCheck.Version?>(null)
    val pending: StateFlow<UpdateCheck.Version?> = _pending

    /** La versión instalada (versionName del paquete). */
    fun current(context: Context): UpdateCheck.Version? = try {
        UpdateCheck.parse(context.packageManager.getPackageInfo(context.packageName, 0).versionName)
    } catch (_: Exception) {
        null
    }

    /** Recalcula el aviso a partir de lo guardado (al arrancar y tras cada consulta). */
    fun refresh(context: Context) {
        val app = context.applicationContext
        _pending.value = UpdateCheck.pending(
            current(app),
            UpdateCheck.parse(AppPrefs.updateLatest(app)),
            UpdateCheck.parse(AppPrefs.updateDismissed(app))
        )
    }

    /** Consulta GitHub si toca (activado y ≥ 24 h); guarda el resultado y refresca el aviso. */
    suspend fun checkIfDue(context: Context) {
        val app = context.applicationContext
        val now = System.currentTimeMillis()
        if (!UpdateCheck.due(AppPrefs.updateCheckEnabled(app), AppPrefs.updateLastMs(app), now)) return
        val v = UpdateClient.latest() ?: return
        AppPrefs.setUpdateResult(app, now, v.toString())
        refresh(app)
    }

    /** «Ocultar»: esa versión no se vuelve a anunciar (una posterior, sí). */
    fun dismiss(context: Context, v: UpdateCheck.Version) {
        val app = context.applicationContext
        AppPrefs.setUpdateDismissed(app, v.toString())
        refresh(app)
    }
}
