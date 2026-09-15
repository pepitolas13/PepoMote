package dev.pepotech.pepomote.server.setup

import java.nio.ByteBuffer
import java.nio.charset.CodingErrorAction
import java.util.Locale

/** Deliberately supports ordinary INI only; ambiguous files are never rewritten. */
internal class IniDocument private constructor(
    private val bom: String,
    private val lines: List<Line>,
    private val newline: String
) {
    private data class Line(val text: String, val ending: String, val section: String? = null, val key: IniKey? = null)

    private val keys = lines.mapNotNull { line -> line.key?.let { canonical(it) to line } }.toMap()
    val sections: Set<String> = lines.mapNotNull { it.section }.toSet()

    fun value(key: IniKey): String? = rawLine(key)?.substringAfter('=')?.trim()
    fun rawLine(key: IniKey): String? = keys[canonical(key)]?.text

    fun merge(values: Map<IniKey, String>): String {
        values.forEach { (key, value) ->
            requireSafeSetting(key, value)
        }
        val remaining = values.entries.associateByTo(linkedMapOf()) { canonical(it.key) }
        val output = mutableListOf<Line>()
        var section = ""

        fun append(text: String) {
            if (output.lastOrNull()?.ending == "") output[output.lastIndex] = output.last().copy(ending = newline)
            output += Line(text, newline)
        }

        fun addMissing(currentSection: String) {
            val missing = remaining.filterValues { it.key.section.equals(currentSection, true) }
            missing.forEach { (id, entry) ->
                append("${entry.key.name} = ${entry.value}")
                remaining.remove(id)
            }
        }

        for (line in lines) {
            line.section?.let {
                addMissing(section)
                section = it
            }
            val replacement = line.key?.let { remaining.remove(canonical(it)) }
            if (replacement == null) {
                output += line
            } else {
                val equals = line.text.indexOf('=')
                val right = line.text.substring(equals + 1)
                val prefix = right.takeWhile { it == ' ' || it == '\t' }
                val suffix = right.drop(prefix.length).takeLastWhile { it == ' ' || it == '\t' }
                output += line.copy(text = line.text.substring(0, equals + 1) + prefix + replacement.value + suffix)
            }
        }
        addMissing(section)
        while (remaining.isNotEmpty()) {
            val nextSection = remaining.values.first().key.section
            if (output.isNotEmpty() && output.last().text.isNotBlank()) append("")
            if (nextSection.isNotEmpty()) append("[$nextSection]")
            addMissing(nextSection)
        }
        return bom + output.joinToString("") { it.text + it.ending }
    }

    /** Restores original spelling/spacing of owned lines while retaining other edits. */
    fun restoreLines(replacements: Map<IniKey, String?>, originallyPresentSections: Set<String>): String {
        val normalized = replacements.mapKeys { canonical(it.key) }
        val output = lines.mapNotNull { line ->
            val id = line.key?.let(::canonical)
            if (id != null && normalized.containsKey(id)) normalized[id]?.let { line.copy(text = it) } else line
        }.toMutableList()
        // Remove an added section only when no content other than whitespace remains in it.
        for (index in output.indices.reversed()) {
            val section = output[index].section ?: continue
            if (originallyPresentSections.any { it.equals(section, true) }) continue
            val end = (index + 1 until output.size).firstOrNull { output[it].section != null } ?: output.size
            if ((index + 1 until end).all { output[it].text.isBlank() }) {
                repeat(end - index) { output.removeAt(index) }
            }
        }
        return bom + output.joinToString("") { it.text + it.ending }
    }

    companion object {
        private fun canonical(key: IniKey) = key.section.lowercase(Locale.ROOT) + '\u0000' + key.name.lowercase(Locale.ROOT)

        fun decode(bytes: ByteArray): String {
            if (bytes.size > SetupSafety.MAX_FILE_BYTES) throw SetupException("The emulator configuration is too large to edit safely.")
            return try {
                Charsets.UTF_8.newDecoder().onMalformedInput(CodingErrorAction.REPORT)
                    .onUnmappableCharacter(CodingErrorAction.REPORT).decode(ByteBuffer.wrap(bytes)).toString()
            } catch (error: java.nio.charset.CharacterCodingException) {
                throw SetupException("The emulator configuration is not valid UTF-8. Open and save its settings in the emulator, then try again.", error)
            }
        }

        fun parse(text: String): IniDocument {
            if (text.length > SetupSafety.MAX_FILE_BYTES || text.any { it < ' ' && it != '\t' && it != '\r' && it != '\n' }) {
                throw SetupException("The emulator configuration contains unsupported or oversized data. Open and save its settings in the emulator first.")
            }
            val bom = if (text.startsWith('\uFEFF')) "\uFEFF" else ""
            val body = text.removePrefix(bom)
            val chunks = mutableListOf<Pair<String, String>>()
            var position = 0
            for (match in Regex("\\r\\n|\\r|\\n").findAll(body)) {
                chunks += body.substring(position, match.range.first) to match.value
                position = match.range.last + 1
            }
            if (position < body.length) chunks += body.substring(position) to ""
            if (chunks.size > 40_000) throw SetupException("The emulator configuration has too many lines to edit safely.")
            var section = ""
            val seenSections = mutableSetOf<String>()
            val seenKeys = mutableSetOf<String>()
            val lines = chunks.mapIndexed { index, (line, ending) ->
                if (line.length > 65_536) throw SetupException("An emulator configuration line is too long to edit safely.")
                val trimmed = line.trim()
                when {
                    trimmed.isEmpty() || trimmed.startsWith(';') || trimmed.startsWith('#') -> Line(line, ending)
                    trimmed.startsWith('[') -> {
                        val match = Regex("^\\[([^\\[\\]]+)]\\s*(?:[;#].*)?$").matchEntire(trimmed)
                            ?: throw SetupException("Invalid INI section on line ${index + 1}. Open and save the emulator settings before trying again.")
                        section = match.groupValues[1].trim()
                        if (section.isEmpty() || !seenSections.add(section.lowercase(Locale.ROOT))) {
                            throw SetupException("Duplicate or empty INI section on line ${index + 1}; no configuration was changed.")
                        }
                        Line(line, ending, section = section)
                    }
                    '=' in line -> {
                        val name = line.substringBefore('=').trim()
                        if (name.isEmpty() || line.substringAfter('=').trimStart().startsWith("<<<")) {
                            throw SetupException("Unsupported INI setting on line ${index + 1}; no configuration was changed.")
                        }
                        val key = IniKey(section, name)
                        if (!seenKeys.add(canonical(key))) throw SetupException("Duplicate INI setting '$name'; no configuration was changed.")
                        Line(line, ending, key = key)
                    }
                    else -> throw SetupException("Invalid INI line ${index + 1}. Open and save the emulator settings before trying again.")
                }
            }
            return IniDocument(bom, lines, chunks.firstOrNull { it.second.isNotEmpty() }?.second ?: "\n")
        }

        private fun requireSafeSetting(key: IniKey, value: String) {
            if (key.section.any { it in "\r\n[]\u0000" } || key.name.isBlank() || key.name.any { it in "\r\n=\u0000" } || value.any { it in "\r\n\u0000" }) {
                throw SetupException("Invalid generated emulator setting.")
            }
        }
    }
}
