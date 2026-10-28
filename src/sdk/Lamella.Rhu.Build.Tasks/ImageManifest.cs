// The image's manifest written beside the image after the link: the flash-image record (schema 1) with the execution model the image runs under (native, the third key), the header's words read from the image's own bytes, the artifact's identity as shipped, the runtime build and the two streams' channels; ASCII, LF, no time, written whole or not at all.
// The header is asserted before anything is written (the magic, the layout, the stated
// load address against the lowest loadable segment), so a record never describes an
// image that is not one.
using System;
using System.Globalization;
using System.IO;
using System.Security.Cryptography;
using System.Text;
using Microsoft.Build.Framework;
using Microsoft.Build.Utilities;

namespace Lamella.Rhu.Build
{
    public sealed class ImageManifest : Task
    {
        [Required]
        public string Image { get; set; } = string.Empty;

        [Required]
        public string Output { get; set; } = string.Empty;

        [Required]
        public string Board { get; set; } = string.Empty;

        [Required]
        public string Runtime { get; set; } = string.Empty;

        public bool RuntimeTested { get; set; }

        [Required]
        public string ConsoleChannel { get; set; } = string.Empty;

        [Required]
        public string LogChannel { get; set; } = string.Empty;

        // The in-image header, layout version 1: sixteen little-endian words at image base + 0x400,
        // riding the first load segment (the kernel's header module is the layout of record).
        private const uint HeaderMagic = 0x49554852;
        private const uint HeaderLayout = 1;
        private const uint HeaderBytes = 64;
        private const uint HeaderOffset = 0x400;
        private const uint PtLoad = 1;

        /// <summary>The header's words a record carries: the layout version, the hosting-API generation and the debug-layout version.</summary>
        private readonly struct Header
        {
            public readonly uint LayoutVersion, Abi, DebugLayoutVersion;
            public Header(uint layout, uint abi, uint debug) { LayoutVersion = layout; Abi = abi; DebugLayoutVersion = debug; }
        }

        public override bool Execute()
        {
            try
            {
                if (!File.Exists(Image))
                {
                    Log.LogError("REFUSE: the image's manifest was not written: no image at {0}", Image);
                    return false;
                }
                byte[] elf = File.ReadAllBytes(Image);
                Header h = ReadHeader(elf);
                string sha;
                using (var sha256 = SHA256.Create())
                {
                    sha = Hex(sha256.ComputeHash(elf));
                }
                string text = Compose(Path.GetFileName(Image), elf.Length, sha, h);
                WriteWhole(Output, text);
                return true;
            }
            catch (Exception e) when (e is IOException || e is UnauthorizedAccessException || e is InvalidOperationException)
            {
                Log.LogError("REFUSE: the image's manifest was not written: {0}", e.Message);
                return false;
            }
        }

        /// <summary>
        /// The header read through the image's program headers: the lowest loadable segment
        /// with file bytes carries it at + 0x400, its stated load address is that segment's
        /// physical address, and its words are the layout's.
        /// </summary>
        private static Header ReadHeader(byte[] elf)
        {
            if (elf.Length < 52 || elf[0] != 0x7f || elf[1] != (byte)'E' || elf[2] != (byte)'L' || elf[3] != (byte)'F')
            {
                throw new InvalidOperationException("the image is not an ELF file");
            }
            if (elf[4] != 1 || elf[5] != 1)
            {
                throw new InvalidOperationException("the image is not a 32-bit little-endian ELF file");
            }
            if (Elf32.Read16(elf, 16) != 2)
            {
                throw new InvalidOperationException("the image is not an executable (ET_EXEC) ELF file");
            }
            if (Elf32.Read16(elf, 18) != 40)
            {
                throw new InvalidOperationException("the image is not an ARM ELF file");
            }
            int phoff = checked((int)Elf32.Read32(elf, 28));
            int phentsize = Elf32.Read16(elf, 42);
            int phnum = Elf32.Read16(elf, 44);
            if (phentsize < 32 || phnum == 0)
            {
                throw new InvalidOperationException("the image's program header table is malformed");
            }
            long lowPaddr = -1;
            long lowOffset = 0;
            long lowFilesz = 0;
            for (int i = 0; i < phnum; i++)
            {
                int at = phoff + i * phentsize;
                uint type = Elf32.Read32(elf, at);
                uint offset = Elf32.Read32(elf, at + 4);
                uint paddr = Elf32.Read32(elf, at + 12);
                uint filesz = Elf32.Read32(elf, at + 16);
                if (type != PtLoad || filesz == 0)
                {
                    continue;
                }
                if (lowPaddr < 0 || paddr < lowPaddr)
                {
                    lowPaddr = paddr;
                    lowOffset = offset;
                    lowFilesz = filesz;
                }
            }
            if (lowPaddr < 0)
            {
                throw new InvalidOperationException("no load segment of the image carries file bytes");
            }
            if (lowFilesz < HeaderOffset + HeaderBytes || lowOffset + HeaderOffset + HeaderBytes > elf.Length)
            {
                throw new InvalidOperationException("the image carries no header magic (the first load segment ends before image base + 0x400)");
            }
            int headerAt = checked((int)(lowOffset + HeaderOffset));
            uint[] w = new uint[16];
            for (int i = 0; i < 16; i++)
            {
                w[i] = Elf32.Read32(elf, headerAt + 4 * i);
            }
            if (w[0] != HeaderMagic)
            {
                throw new InvalidOperationException(string.Format(CultureInfo.InvariantCulture, "the image carries no header magic at image base + 0x400 (read 0x{0:x8}, wanted 0x{1:x8})", w[0], HeaderMagic));
            }
            if (w[1] != HeaderLayout || w[2] != HeaderBytes)
            {
                throw new InvalidOperationException(string.Format(CultureInfo.InvariantCulture, "the image header's layout is {0} / {1} bytes, this SDK reads layout {2} / {3} bytes", w[1], w[2], HeaderLayout, HeaderBytes));
            }
            if (w[3] != (uint)lowPaddr)
            {
                throw new InvalidOperationException(string.Format(CultureInfo.InvariantCulture, "the image header states the load address 0x{0:x8}, the lowest loadable segment sits at 0x{1:x8}", w[3], lowPaddr));
            }
            for (int i = 6; i < 16; i++)
            {
                if (w[i] != 0)
                {
                    throw new InvalidOperationException("the image header's reserved words are not zero");
                }
            }
            return new Header(w[1], w[4], w[5]);
        }

        /// <summary>The record, in the contract's key order; two-space indentation, LF, a final newline.</summary>
        private string Compose(string imageName, int bytes, string sha, Header h)
        {
            var sb = new StringBuilder();
            sb.Append("{\n");
            sb.Append("  \"schema\": 1,\n");
            sb.Append("  \"kind\": \"flash-image\",\n");
            sb.Append("  \"model\": \"native\",\n");
            sb.Append("  \"board\": ").Append(Quote(Board)).Append(",\n");
            sb.Append("  \"format\": \"elf\",\n");
            sb.Append("  \"base\": null,\n");
            sb.Append("  \"sha256\": \"").Append(sha).Append("\",\n");
            sb.Append("  \"bytes\": ").Append(bytes.ToString(CultureInfo.InvariantCulture)).Append(",\n");
            sb.Append("  \"producer\": \"ilc+rhu-link\",\n");
            sb.Append("  \"elf\": ").Append(Quote(imageName)).Append(",\n");
            sb.Append("  \"map\": null,\n");
            sb.Append("  \"image_kind\": \"monolith\",\n");
            sb.Append("  \"layout_version\": ").Append(h.LayoutVersion.ToString(CultureInfo.InvariantCulture)).Append(",\n");
            sb.Append("  \"abi\": ").Append(h.Abi.ToString(CultureInfo.InvariantCulture)).Append(",\n");
            sb.Append("  \"debug_layout_version\": ").Append(h.DebugLayoutVersion.ToString(CultureInfo.InvariantCulture)).Append(",\n");
            sb.Append("  \"codegen_profile\": \"ship\",\n");
            sb.Append("  \"runtime\": ").Append(Quote(Runtime)).Append(",\n");
            sb.Append("  \"runtime_tested\": ").Append(RuntimeTested ? "true" : "false").Append(",\n");
            sb.Append("  \"console_channel\": ").Append(Quote(ConsoleChannel)).Append(",\n");
            sb.Append("  \"log_channel\": ").Append(Quote(LogChannel)).Append("\n");
            sb.Append("}\n");
            return sb.ToString();
        }

        /// <summary>A JSON string of ASCII alone: the quote, the backslash, the control characters and every byte past ASCII escaped.</summary>
        private static string Quote(string s)
        {
            var sb = new StringBuilder(s.Length + 2);
            sb.Append('"');
            foreach (char c in s)
            {
                if (c == '"' || c == '\\')
                {
                    sb.Append('\\').Append(c);
                }
                else if (c < 0x20 || c > 0x7e)
                {
                    sb.Append("\\u").Append(((int)c).ToString("x4", CultureInfo.InvariantCulture));
                }
                else
                {
                    sb.Append(c);
                }
            }
            sb.Append('"');
            return sb.ToString();
        }

        private static string Hex(byte[] bytes)
        {
            var sb = new StringBuilder(bytes.Length * 2);
            foreach (byte b in bytes)
            {
                sb.Append(b.ToString("x2", CultureInfo.InvariantCulture));
            }
            return sb.ToString();
        }

        /// <summary>The file written beside its final name and moved into place, so a reader never sees a partial record.</summary>
        private static void WriteWhole(string path, string text)
        {
            string tmp = path + ".tmp";
            File.WriteAllBytes(tmp, Encoding.ASCII.GetBytes(text));
            File.Move(tmp, path, true);
        }
    }
}
