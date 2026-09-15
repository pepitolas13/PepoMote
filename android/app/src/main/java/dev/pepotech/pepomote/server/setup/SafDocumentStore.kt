package dev.pepotech.pepomote.server.setup

import android.content.ContentResolver
import android.database.Cursor
import android.net.Uri
import android.os.Bundle
import android.provider.DocumentsContract
import java.io.ByteArrayOutputStream

/** The only emulator filesystem adapter. All child IDs come from provider queries. */
internal class SafDocumentStore(
    private val resolver: ContentResolver,
    private val target: EmulatorTarget,
    private val treeUri: Uri,
    val packageName: String
) : SetupDocumentStore {
    private data class Document(val id: String, val name: String, val mime: String, val flags: Int, val size: Long)
    private data class Snapshot(val documentId: String?, val bytes: ByteArray?)
    private val authority = treeUri.authority ?: throw SetupException("The selected folder has no document provider.")
    private val treeId = DocumentsContract.getTreeDocumentId(treeUri)
    private val configName = SetupSafety.configDirectory(target)
    private val selectedIsConfig = SetupSafety.validateRoot(target, authority, packageName, treeId).isEmpty()
    private val snapshots = mutableMapOf<String, Snapshot>()

    init {
        selectedDirectory()
    }

    override fun read(path: String): ByteArray? {
        SetupSafety.validatePath(target, path)
        val document = resolve(path, createParents = false)
        val bytes = document?.let(::readBytes)
        snapshots[path] = Snapshot(document?.id, bytes)
        return bytes
    }

    override fun write(path: String, bytes: ByteArray) {
        SetupSafety.validatePath(target, path)
        if (bytes.size > SetupSafety.MAX_FILE_BYTES) throw SetupException("The generated emulator configuration is too large.")
        var document = resolve(path, createParents = true)
        if (document == null) {
            if (snapshots[path]?.documentId != null) {
                throw SetupException("$path disappeared during setup. Close the emulator and try again.")
            }
            val parts = path.split('/')
            var parent = configDirectory(create = true) ?: throw SetupException("The configuration folder is unavailable.")
            for (part in parts.dropLast(1)) parent = directoryChild(parent, part, create = true)!!
            document = create(parent, parts.last(), "application/octet-stream")
            if (document.flags and DocumentsContract.Document.FLAG_SUPPORTS_DELETE == 0) {
                throw SetupException("The provider cannot remove a new profile during restoration. No profile contents were written.")
            }
        } else {
            requireFile(document)
            val observed = snapshots[path]
            if (observed != null) {
                if (observed.documentId != document.id || !equalBytes(observed.bytes, readBytes(document))) {
                    throw SetupException("$path changed during setup. Close the emulator and try again.")
                }
            }
        }
        if (document.flags and DocumentsContract.Document.FLAG_SUPPORTS_WRITE == 0) {
            throw SetupException("The emulator has not granted write access to $path. Select its folder again with read and write permission.")
        }
        // rwt is essential: a shorter INI must not leave bytes from its previous contents.
        val output = resolver.openOutputStream(uri(document), "rwt")
            ?: throw SetupException("The emulator did not open $path for writing.")
        output.use {
            it.write(bytes)
            it.flush()
        }
    }

    override fun delete(path: String) {
        SetupSafety.validatePath(target, path)
        val document = resolve(path, createParents = false) ?: return
        requireFile(document) // Never call a provider's recursive directory delete.
        val observed = snapshots[path]
        if (observed != null &&
            (observed.documentId != document.id || !equalBytes(observed.bytes, readBytes(document)))) {
            throw SetupException("$path changed before restoration. It has been kept.")
        }
        if (document.flags and DocumentsContract.Document.FLAG_SUPPORTS_DELETE == 0 ||
            !DocumentsContract.deleteDocument(resolver, uri(document))) {
            throw SetupException("The emulator could not remove the unchanged generated file $path.")
        }
    }

    private fun selectedDirectory(): Document {
        val selected = query(DocumentsContract.buildDocumentUriUsingTree(treeUri, treeId))
        SetupSafety.validateRoot(target, authority, packageName, selected.id)
        if (selected.id != treeId && !(treeId == "root" && selected.id == "root/")) {
            throw SetupException("The document provider returned a different selected folder.")
        }
        requireDirectory(selected)
        return selected
    }

    private fun configDirectory(create: Boolean): Document? {
        val selected = selectedDirectory()
        return if (selectedIsConfig) selected else directoryChild(selected, configName, create)
    }

    private fun resolve(path: String, createParents: Boolean): Document? {
        val parts = path.split('/')
        var parent = configDirectory(createParents) ?: return null
        for (part in parts.dropLast(1)) parent = directoryChild(parent, part, createParents) ?: return null
        return child(parent, parts.last())?.also(::requireFile)
    }

    private fun directoryChild(parent: Document, name: String, create: Boolean): Document? {
        val child = child(parent, name) ?: if (create) create(parent, name, DocumentsContract.Document.MIME_TYPE_DIR) else return null
        requireDirectory(child)
        return child
    }

    private fun child(parent: Document, name: String): Document? {
        requireDirectory(parent)
        val childrenUri = DocumentsContract.buildChildDocumentsUriUsingTree(treeUri, parent.id)
        var found: Document? = null
        resolver.query(childrenUri, PROJECTION, Bundle.EMPTY, null)?.use { cursor ->
            var count = 0
            while (cursor.moveToNext()) {
                if (++count > 10_000) throw SetupException("The emulator configuration folder has too many entries to inspect safely.")
                val candidateName = cursor.getString(cursor.getColumnIndexOrThrow(DocumentsContract.Document.COLUMN_DISPLAY_NAME))
                if (candidateName != name) continue
                if (found != null) throw SetupException("The emulator provider returned duplicate names for $name.")
                found = metadata(cursor).also { validateChild(parent, it, name) }
            }
        } ?: throw SetupException("The emulator folder could not be listed. Open the emulator once and select its folder again.")
        return found
    }

    private fun create(parent: Document, name: String, mime: String): Document {
        if (parent.flags and DocumentsContract.Document.FLAG_DIR_SUPPORTS_CREATE == 0) {
            throw SetupException("The emulator folder does not allow creating configuration files. Select it again with write permission.")
        }
        val createdUri = DocumentsContract.createDocument(resolver, uri(parent), mime, name)
            ?: throw SetupException("The emulator could not create $name.")
        if (createdUri.scheme != "content" || createdUri.authority != authority) {
            throw SetupException("The emulator returned an unexpected document provider when creating $name.")
        }
        val createdId = DocumentsContract.getDocumentId(createdUri)
        // Query through the actual granted tree even when createDocument returns a plain document URI.
        val created = query(DocumentsContract.buildDocumentUriUsingTree(treeUri, createdId))
        validateChild(parent, created, name)
        if (created.mime != mime && (mime == DocumentsContract.Document.MIME_TYPE_DIR || created.mime == DocumentsContract.Document.MIME_TYPE_DIR)) {
            throw SetupException("The emulator created an unexpected file type for $name.")
        }
        return created
    }

    private fun validateChild(parent: Document, child: Document, expectedName: String) {
        val expectedId = parent.id.trimEnd('/') + "/" + expectedName
        // This verifies the provider's audited path format; it does not manufacture child URIs.
        if (child.name != expectedName || child.id != expectedId || child.id.length > 512 ||
            expectedName.any { it == '/' || it == '\\' || it == '\u0000' } || expectedName in listOf(".", "..")) {
            throw SetupException("The emulator returned an unexpected path or a renamed file for $expectedName. No different file will be edited.")
        }
    }

    private fun query(uri: Uri): Document = resolver.query(uri, PROJECTION, Bundle.EMPTY, null)?.use { cursor ->
        if (!cursor.moveToFirst()) throw SetupException("The selected emulator folder is unavailable. Open the emulator once, then select its folder again.")
        val document = metadata(cursor)
        if (cursor.moveToNext()) throw SetupException("The emulator returned an ambiguous document.")
        document
    } ?: throw SetupException("The selected emulator folder cannot be opened.")

    private fun metadata(cursor: Cursor): Document {
        fun string(column: String) = cursor.getString(cursor.getColumnIndexOrThrow(column))
            ?: throw SetupException("The emulator returned incomplete document information.")
        val sizeIndex = cursor.getColumnIndexOrThrow(DocumentsContract.Document.COLUMN_SIZE)
        return Document(
            string(DocumentsContract.Document.COLUMN_DOCUMENT_ID),
            string(DocumentsContract.Document.COLUMN_DISPLAY_NAME),
            string(DocumentsContract.Document.COLUMN_MIME_TYPE),
            cursor.getInt(cursor.getColumnIndexOrThrow(DocumentsContract.Document.COLUMN_FLAGS)),
            if (cursor.isNull(sizeIndex)) -1 else cursor.getLong(sizeIndex)
        )
    }

    private fun readBytes(document: Document): ByteArray {
        requireFile(document)
        if (document.size > SetupSafety.MAX_FILE_BYTES) throw SetupException("${document.name} is too large to edit safely.")
        return resolver.openInputStream(uri(document))?.use { input ->
            val output = ByteArrayOutputStream()
            val buffer = ByteArray(8192)
            while (true) {
                val count = input.read(buffer)
                if (count == -1) break
                if (output.size() + count > SetupSafety.MAX_FILE_BYTES) throw SetupException("${document.name} is too large to edit safely.")
                output.write(buffer, 0, count)
            }
            output.toByteArray()
        } ?: throw SetupException("The emulator did not open ${document.name} for reading.")
    }

    private fun uri(document: Document) = DocumentsContract.buildDocumentUriUsingTree(treeUri, document.id)
    private fun requireDirectory(document: Document) {
        if (document.mime != DocumentsContract.Document.MIME_TYPE_DIR) throw SetupException("${document.name} must be an emulator folder.")
    }
    private fun requireFile(document: Document) {
        if (document.mime == DocumentsContract.Document.MIME_TYPE_DIR) throw SetupException("${document.name} is a folder, not a configuration file; it will not be changed.")
    }
    private fun equalBytes(a: ByteArray?, b: ByteArray?) = if (a == null || b == null) a == null && b == null else a.contentEquals(b)

    companion object {
        private val PROJECTION = arrayOf(
            DocumentsContract.Document.COLUMN_DOCUMENT_ID,
            DocumentsContract.Document.COLUMN_DISPLAY_NAME,
            DocumentsContract.Document.COLUMN_MIME_TYPE,
            DocumentsContract.Document.COLUMN_FLAGS,
            DocumentsContract.Document.COLUMN_SIZE
        )
    }
}
