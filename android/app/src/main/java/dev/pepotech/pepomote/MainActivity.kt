package dev.pepotech.pepomote

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import dev.pepotech.pepomote.ui.screens.HomeScreen
import dev.pepotech.pepomote.ui.theme.PepoMoteTheme

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent {
            PepoMoteTheme {
                HomeScreen()
            }
        }
    }
}
