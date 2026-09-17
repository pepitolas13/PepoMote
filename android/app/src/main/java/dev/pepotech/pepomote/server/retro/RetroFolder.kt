package dev.pepotech.pepomote.server.retro

import android.content.ContentResolver
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.provider.DocumentsContract

/**
 * La carpeta `RetroArch` de la memoria del móvil (donde RetroArch guarda
 * `playlists/builtin/content_history.lpl`), elegida una vez con el selector
 * de carpetas de Android y recordada. Solo lectura: PepoMote no escribe ahí.
 *
 * RetroArch guarda su configuración en `Android/data/com.retroarch/files`,
 * que ninguna otra app puede leer ni escribir en Android 11 y posteriores;
 * por eso el mando en red se activa en el propio RetroArch (ver la guía) y
 * aquí solo se lee el historial, que sí vive en la carpeta pública.
 */
object RetroFolder {
    private const val PREFS = "retroarch_folder"
    private const val KEY_TREE = "tree"
    private const val EXTERNAL_AUTHORITY = "com.android.externalstorage.documents"
    private const val FILE = "content_history.lpl"
    private const val MAX_BYTES = 4 * 1024 * 1024
    /** Rutas relativas donde RetroArch deja el historial (1.22: `playlists/builtin`; antes, la raíz). */
    private val CANDIDATES = listOf(listOf("playlists", "builtin", FILE), listOf("playlists", FILE), listOf(FILE))

    /** Dónde abrir el selector: la carpeta RetroArch de la memoria interna (Android 8+ la respeta si existe). */
    fun initialUri(): Uri = DocumentsContract.buildDocumentUri(EXTERNAL_AUTHORITY, "primary:RetroArch")

    fun pickerIntent(): Intent = Intent(Intent.ACTION_OPEN_DOCUMENT_TREE)
        .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION)
        .putExtra(DocumentsContract.EXTRA_INITIAL_URI, initialUri())

    /** La carpeta guardada, solo si Android sigue dejándola leer. */
    fun granted(context: Context): Uri? {
        val app = context.applicationContext
        val saved = app.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getString(KEY_TREE, null)?.let(Uri::parse) ?: return null
        val kept = runCatching { app.contentResolver.persistedUriPermissions.any { it.uri == saved && it.isReadPermission } }.getOrDefault(false)
        return if (kept) saved else null
    }

    /** Guarda la carpeta elegida (conserva el permiso). Devuelve false si Android no lo concede. */
    fun save(context: Context, tree: Uri): Boolean {
        if (tree.scheme != "content" || !DocumentsContract.isTreeUri(tree)) return false
        val app = context.applicationContext
        try {
            app.contentResolver.takePersistableUriPermission(tree, Intent.FLAG_GRANT_READ_URI_PERMISSION)
        } catch (_: SecurityException) {
            return false
        }
        forget(app, keepSaved = tree)
        app.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().putString(KEY_TREE, tree.toString()).apply()
        return true
    }

    /** Suelta el permiso (y el recuerdo) de la carpeta. */
    fun forget(context: Context, keepSaved: Uri? = null) {
        val app = context.applicationContext
        val prefs = app.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        val saved = prefs.getString(KEY_TREE, null)?.let(Uri::parse)
        if (saved != null && saved != keepSaved) {
            runCatching { app.contentResolver.releasePersistableUriPermission(saved, Intent.FLAG_GRANT_READ_URI_PERMISSION) }
        }
        if (keepSaved == null) prefs.edit().remove(KEY_TREE).apply()
    }

    /** Fuente del historial dentro de la carpeta: busca el archivo por su ruta relativa. */
    fun source(resolver: ContentResolver, tree: Uri): HistorySource = HistorySource {
        val rootId = DocumentsContract.getTreeDocumentId(tree)
        for (candidate in CANDIDATES) {
            var parent = rootId
            var found: String? = null
            for ((i, name) in candidate.withIndex()) {
                val child = child(resolver, tree, parent, name, directory = i < candidate.lastIndex) ?: break
                if (i == candidate.lastIndex) found = child else parent = child
            }
            if (found != null) return@HistorySource SafDocument(resolver, DocumentsContract.buildDocumentUriUsingTree(tree, found))
        }
        null
    }

    private fun child(resolver: ContentResolver, tree: Uri, parentId: String, name: String, directory: Boolean): String? {
        val children = DocumentsContract.buildChildDocumentsUriUsingTree(tree, parentId)
        return runCatching {
            resolver.query(children, PROJECTION, Bundle.EMPTY, null)?.use { cursor ->
                val idColumn = cursor.getColumnIndexOrThrow(DocumentsContract.Document.COLUMN_DOCUMENT_ID)
                val nameColumn = cursor.getColumnIndexOrThrow(DocumentsContract.Document.COLUMN_DISPLAY_NAME)
                val mimeColumn = cursor.getColumnIndexOrThrow(DocumentsContract.Document.COLUMN_MIME_TYPE)
                var count = 0
                while (cursor.moveToNext()) {
                    if (++count > 20_000) return@use null
                    if (cursor.getString(nameColumn) != name) continue
                    val isDir = cursor.getString(mimeColumn) == DocumentsContract.Document.MIME_TYPE_DIR
                    if (isDir == directory) return@use cursor.getString(idColumn)
                }
                null
            }
        }.getOrNull()
    }

    private class SafDocument(private val resolver: ContentResolver, private val uri: Uri) : HistoryDocument {
        override fun stamp(): Pair<Long, Long>? = runCatching {
            resolver.query(uri, STAMP, Bundle.EMPTY, null)?.use { cursor ->
                if (!cursor.moveToFirst()) return@use null
                val modified = cursor.getColumnIndex(DocumentsContract.Document.COLUMN_LAST_MODIFIED).takeIf { it >= 0 }?.let { if (cursor.isNull(it)) 0L else cursor.getLong(it) } ?: 0L
                val size = cursor.getColumnIndex(DocumentsContract.Document.COLUMN_SIZE).takeIf { it >= 0 }?.let { if (cursor.isNull(it)) -1L else cursor.getLong(it) } ?: -1L
                modified to size
            }
        }.getOrNull()

        override fun read(): String? = runCatching {
            resolver.openInputStream(uri)?.use { input ->
                val bytes = input.readBytes()
                if (bytes.size > MAX_BYTES) null else String(bytes, Charsets.UTF_8)
            }
        }.getOrNull()
    }

    private val PROJECTION = arrayOf(
        DocumentsContract.Document.COLUMN_DOCUMENT_ID,
        DocumentsContract.Document.COLUMN_DISPLAY_NAME,
        DocumentsContract.Document.COLUMN_MIME_TYPE,
    )
    private val STAMP = arrayOf(DocumentsContract.Document.COLUMN_LAST_MODIFIED, DocumentsContract.Document.COLUMN_SIZE)
}
