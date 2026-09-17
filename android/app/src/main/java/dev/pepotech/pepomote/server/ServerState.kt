package dev.pepotech.pepomote.server

import dev.pepotech.pepomote.server.core.ReceiverConfig
import dev.pepotech.pepomote.server.core.ReceiverSnapshot
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow

/** La carpeta RetroArch enlazada: si se ha encontrado en ella el historial. */
data class RetroFolderStatus(val located: Boolean)

data class ServerUiState(
    val starting: Boolean = false,
    val config: ReceiverConfig? = null,
    val hosts: List<String> = emptyList(),
    val receiver: ReceiverSnapshot? = null,
    val error: String? = null,
    val vpnHosts: List<String> = emptyList(),
    /** null = sin carpeta RetroArch enlazada (o servidor parado). */
    val retroFolder: RetroFolderStatus? = null,
) {
    val running: Boolean get() = receiver?.running == true
    val active: Boolean get() = starting || running
}

object ServerState {
    internal val mutable = MutableStateFlow(ServerUiState())
    val flow = mutable.asStateFlow()
}
