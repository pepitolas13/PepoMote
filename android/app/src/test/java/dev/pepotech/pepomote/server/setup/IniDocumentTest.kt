package dev.pepotech.pepomote.server.setup

import org.junit.Assert.*
import org.junit.Test

class IniDocumentTest {
    @Test fun mergeKeepsBomCrLfCasingCommentsAndUnrelatedSections() {
        val before = "\uFEFF; personal settings\r\n[Server]\r\n  eNabled  =  False  \r\nEntries = Other:192.168.1.8:26760;\r\n\r\n[Video]\r\nBackend = Vulkan\r\n"
        val after = IniDocument.parse(before).merge(linkedMapOf(IniKey("Server", "Enabled") to "True"))
        assertEquals(before.replace("False", "True"), after)
    }

    @Test fun addedKeysStayInTheirSectionWithoutChangingAdjacentSection() {
        val before = "# note\n[Server]\nEnabled=False\n[Video]\nBackend=Vulkan"
        val after = IniDocument.parse(before).merge(linkedMapOf(IniKey("Server", "Entries") to "PepoMote:127.0.0.1:26760;"))
        assertEquals("# note\n[Server]\nEnabled=False\nEntries = PepoMote:127.0.0.1:26760;\n[Video]\nBackend=Vulkan", after)
    }

    @Test fun managedDuplicateKeysAndSectionsAreRejectedBeforeEditing() {
        for (input in listOf("[Server]\nEnabled=False\nenabled=True\n", "[Server]\nEnabled=False\n[server]\nEntries=x\n")) {
            assertThrows(SetupException::class.java) { IniDocument.parse(input) }
        }
    }

    @Test fun malformedSectionBinaryDataAndMultilineValuesAreRejected() {
        for (input in listOf("[Server\nEnabled=False\n", "[Server]\n\u0000\n", "[Controls]\nkey=<<<END\nmulti line\nEND\n")) {
            assertThrows(SetupException::class.java) { IniDocument.parse(input) }
        }
    }

    @Test fun equalsAndSemicolonsInsideValuesAreNotParsedAsCommentsOrSeparators() {
        val ini = IniDocument.parse("[Server]\nEntries = Other:192.168.1.8:26760;PepoMote:127.0.0.1:26760;\nExpression = if(A=B, 1, 0)\n")
        assertEquals("Other:192.168.1.8:26760;PepoMote:127.0.0.1:26760;", ini.value(IniKey("Server", "Entries")))
        assertEquals("if(A=B, 1, 0)", ini.value(IniKey("Server", "Expression")))
    }
}
