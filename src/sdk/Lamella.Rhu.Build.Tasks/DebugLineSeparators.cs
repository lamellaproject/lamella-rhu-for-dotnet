// The image's line tables after the link: the directory and file-name strings of every DWARF line-table header of version 2, 3 or 4 in .debug_line rewritten in place with the forward slash for the backslash, no other byte touched and no length moved, so that the image is the same bytes on a Windows host as on a Linux one. The compiler splits each document name with the host's path functions, so on Windows the directory strings carry the host's separator while every other byte of the image ties. A version-5 unit is left whole. A section that is absent or whose units do not parse refuses the publish by name, so an image never ships with line tables this task could not read.
using System;
using System.Globalization;
using System.IO;
using Microsoft.Build.Framework;
using Microsoft.Build.Utilities;

namespace Lamella.Rhu.Build
{
    public sealed class DebugLineSeparators : Task
    {
        /// <summary>The linked image, rewritten in place where a string carries a backslash.</summary>
        [Required]
        public string Image { get; set; } = string.Empty;

        /// <summary>The count of units read, the version-5 units among them.</summary>
        [Output]
        public int Units { get; set; }

        /// <summary>The count of version-5 units left whole.</summary>
        [Output]
        public int Skipped { get; set; }

        /// <summary>The count of separators rewritten.</summary>
        [Output]
        public int Rewritten { get; set; }

        private const string Code = "RHU5002";

        public override bool Execute()
        {
            try
            {
                byte[] elf = File.ReadAllBytes(Image);
                var (offset, size) = Elf32.FindSection(elf, ".debug_line", 1);
                int end = offset + size;
                int at = offset;
                int units = 0, skipped = 0, rewritten = 0;
                while (at < end)
                {
                    int unitAt = at - offset;
                    if (end - at < 4)
                    {
                        throw Fault(unitAt, "a unit header is cut off at the section's end");
                    }
                    uint unitLength = Elf32.Read32(elf, at);
                    if (unitLength == 0xffffffffu)
                    {
                        throw Fault(unitAt, "a 64-bit unit; this task reads 32-bit line tables");
                    }
                    int unitStart = at + 4;
                    long unitEnd = (long)unitStart + unitLength;
                    if (unitLength < 2 || unitEnd > end)
                    {
                        throw Fault(unitAt, string.Format(CultureInfo.InvariantCulture, "the unit states a length of {0} bytes, past the section's end", unitLength));
                    }
                    ushort version = Elf32.Read16(elf, unitStart);
                    if (version == 5)
                    {
                        skipped++;
                        units++;
                        at = (int)unitEnd;
                        continue;
                    }
                    if (version < 2 || version > 5)
                    {
                        throw Fault(unitAt, string.Format(CultureInfo.InvariantCulture, "the unit is of version {0}; this task reads versions 2 to 5", version));
                    }
                    int p = unitStart + 2;
                    uint headerLength = Elf32.Read32(elf, p);
                    p += 4;
                    long headerEnd = (long)p + headerLength;
                    if (headerEnd > unitEnd)
                    {
                        throw Fault(unitAt, "the header's stated length runs past the unit's end");
                    }
                    int limit = (int)headerEnd;
                    // minimum_instruction_length, maximum_operations_per_instruction (version 4),
                    // default_is_stmt, line_base, line_range, opcode_base
                    p += version >= 4 ? 5 : 4;
                    if (p >= limit)
                    {
                        throw Fault(unitAt, "the header ends before its opcode base");
                    }
                    byte opcodeBase = elf[p];
                    p += 1;
                    if (opcodeBase == 0)
                    {
                        throw Fault(unitAt, "the header's opcode base is zero");
                    }
                    p += opcodeBase - 1;
                    if (p > limit)
                    {
                        throw Fault(unitAt, "the standard opcode lengths run past the header");
                    }
                    while (true)
                    {
                        if (p >= limit)
                        {
                            throw Fault(unitAt, "the include_directories table runs past the header");
                        }
                        if (elf[p] == 0)
                        {
                            p++;
                            break;
                        }
                        p = RewriteString(elf, p, limit, unitAt, ref rewritten);
                    }
                    while (true)
                    {
                        if (p >= limit)
                        {
                            throw Fault(unitAt, "the file_names table runs past the header");
                        }
                        if (elf[p] == 0)
                        {
                            p++;
                            break;
                        }
                        p = RewriteString(elf, p, limit, unitAt, ref rewritten);
                        p = SkipUleb(elf, p, limit, unitAt);
                        p = SkipUleb(elf, p, limit, unitAt);
                        p = SkipUleb(elf, p, limit, unitAt);
                    }
                    units++;
                    at = (int)unitEnd;
                }
                if (units == 0)
                {
                    throw new InvalidOperationException("the .debug_line section holds no unit");
                }
                if (rewritten > 0)
                {
                    string tmp = Image + ".tmp";
                    File.WriteAllBytes(tmp, elf);
                    File.Move(tmp, Image, true);
                }
                Units = units;
                Skipped = skipped;
                Rewritten = rewritten;
                Log.LogMessage(MessageImportance.Low, "Lamella RHU: the line tables of {0}: {1} units read ({2} of version 5 left whole), {3} separators rewritten", Image, units, skipped, rewritten);
                return true;
            }
            catch (Exception e) when (e is IOException || e is UnauthorizedAccessException || e is InvalidOperationException)
            {
                Log.LogError(null, Code, null, Image, 0, 0, 0, 0, "REFUSE: the image's line tables were not read ({0}): the image is not shipped", e.Message);
                return false;
            }
        }

        private static InvalidOperationException Fault(int unitAt, string what)
        {
            return new InvalidOperationException(string.Format(CultureInfo.InvariantCulture, ".debug_line unit at offset {0}: {1}", unitAt, what));
        }

        /// <summary>The string at p rewritten up to its NUL inside the header; the index after the NUL.</summary>
        private static int RewriteString(byte[] elf, int p, int limit, int unitAt, ref int rewritten)
        {
            while (true)
            {
                if (p >= limit)
                {
                    throw Fault(unitAt, "a string runs past the header");
                }
                if (elf[p] == 0)
                {
                    return p + 1;
                }
                if (elf[p] == 0x5c)
                {
                    elf[p] = 0x2f;
                    rewritten++;
                }
                p++;
            }
        }

        /// <summary>An unsigned LEB128 value skipped; the index after its last byte.</summary>
        private static int SkipUleb(byte[] elf, int p, int limit, int unitAt)
        {
            for (int n = 0; n < 10; n++)
            {
                if (p >= limit)
                {
                    throw Fault(unitAt, "a file entry's number runs past the header");
                }
                byte b = elf[p++];
                if ((b & 0x80) == 0)
                {
                    return p;
                }
            }
            throw Fault(unitAt, "a file entry's number is longer than ten bytes");
        }
    }
}
