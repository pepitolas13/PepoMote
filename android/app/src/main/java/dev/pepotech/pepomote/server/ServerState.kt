package dev.pepotech.pepomote.server

import dev.pepotech.pepomote.server.core.ReceiverConfig
import dev.pepotech.pepomote.server.core.ReceiverSnapshot
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow

data class ServerUiState(
    val starting: Boolean = false,
    val config: ReceiverConfig? = null,
    val hosts: List<String> = emptyList(),
    val receiver: ReceiverSnapshot? = null,
    val error: String? = null,
    val vpnHosts: List<String> = emptyList(),
) {
    val running: Boolean get() = receiver?.running == true
    val active: Boolean get() = starting || running
}

object ServerState {
    internal val mutable = MutableStateFlow(ServerUiState())
    val flow = mutable.asStateFlow()
}
