// The program's own code for the native compiler: each listed assembly copied into one directory with every method that has an IL body marked not to be optimized and not to be inlined (two bits of the method's implementation flags), its symbol file copied beside it, so that the native compiler, which runs with the code optimization on for the framework and every package, compiles the program's own methods unoptimized, each a function of its own. The assembly the C# compiler wrote is never touched. Each row of the method table is read back through the metadata reader before a byte of it is changed; a file that does not read as a .NET assembly is refused by name before anything is written; a copy is rewritten only when its bytes moved, so an unchanged build hands the native compiler unchanged files.
using System;
using System.Buffers.Binary;
using System.Collections.Generic;
using System.IO;
using System.Reflection.Metadata;
using System.Reflection.Metadata.Ecma335;
using System.Reflection.PortableExecutable;
using Microsoft.Build.Framework;
using Microsoft.Build.Utilities;

namespace Lamella.Rhu.Build
{
    public sealed class MarkUnoptimized : Task
    {
        /// <summary>The assemblies to mark, each named by its path.</summary>
        [Required]
        public ITaskItem[] Assemblies { get; set; } = Array.Empty<ITaskItem>();

        /// <summary>The directory the marked copies are written into, as the items that name the copies will carry it.</summary>
        [Required]
        public string Directory { get; set; } = string.Empty;

        /// <summary>The marked copies, one per assembly in the same order, each carrying its source item's metadata.</summary>
        [Output]
        public ITaskItem[] Marked { get; set; } = Array.Empty<ITaskItem>();

        // The method's implementation flags: NoInlining 0x0008 and NoOptimization 0x0040; the code type in the low two
        // bits, IL being 0; AggressiveOptimization 0x0200, which a program sets on a method it wants optimized.
        private const ushort MarkBits = 0x0048;
        private const ushort CodeTypeMask = 0x0003;
        private const ushort AggressiveOptimization = 0x0200;

        public override bool Execute()
        {
            try
            {
                string dir = Directory;
                if (dir.Length == 0)
                {
                    Log.LogError("REFUSE: the program's own code was not marked: no directory was named for the marked copies");
                    return false;
                }
                if (!dir.EndsWith('/') && !dir.EndsWith('\\'))
                {
                    dir += "/";
                }
                var names = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
                var marked = new List<ITaskItem>();
                foreach (ITaskItem item in Assemblies)
                {
                    string source = item.ItemSpec;
                    string name = Path.GetFileName(source);
                    string full = Path.GetFullPath(source);
                    if (names.TryGetValue(name, out string? seen))
                    {
                        if (string.Equals(seen, full, OperatingSystem.IsWindows() ? StringComparison.OrdinalIgnoreCase : StringComparison.Ordinal))
                        {
                            continue;
                        }
                        Log.LogError("REFUSE: the program's own code was not marked: two of its assemblies are named {0} ({1} and {2})", name, seen, full);
                        return false;
                    }
                    names.Add(name, full);
                    if (!File.Exists(source))
                    {
                        Log.LogError("REFUSE: the program's own code was not marked: no assembly at {0}", source);
                        return false;
                    }
                    byte[] bytes = File.ReadAllBytes(source);
                    if (!Mark(bytes, source, out int withIl, out int kept))
                    {
                        return false;
                    }
                    System.IO.Directory.CreateDirectory(dir);
                    string target = dir + name;
                    WriteWhenDifferent(target, bytes);
                    string symbols = Path.ChangeExtension(source, ".pdb");
                    if (File.Exists(symbols))
                    {
                        WriteWhenDifferent(Path.ChangeExtension(target, ".pdb"), File.ReadAllBytes(symbols));
                    }
                    var copy = new TaskItem(target);
                    item.CopyMetadataTo(copy);
                    marked.Add(copy);
                    Log.LogMessage(MessageImportance.Low, "Lamella RHU: {0}: {1} methods with an IL body, {2} marked, {3} left optimized as the program marks them", name, withIl, withIl - kept, kept);
                }
                Marked = marked.ToArray();
                return true;
            }
            catch (Exception e) when (e is IOException || e is UnauthorizedAccessException || e is ArgumentException || e is NotSupportedException)
            {
                Log.LogError("REFUSE: the program's own code was not marked: {0}", e.Message);
                return false;
            }
        }

        /// <summary>Sets the two bits in every row of the method table that has an IL body, in the bytes given; false, with the refusal logged, when the bytes do not read as a .NET assembly.</summary>
        private bool Mark(byte[] bytes, string source, out int withIl, out int kept)
        {
            withIl = 0;
            kept = 0;
            try
            {
                using var pe = new PEReader(new MemoryStream((byte[])bytes.Clone(), writable: false));
                if (!pe.HasMetadata)
                {
                    Log.LogError("REFUSE: the program's own code was not marked: {0} is not a .NET assembly (it carries no metadata)", source);
                    return false;
                }
                MetadataReader md = pe.GetMetadataReader();
                int at = pe.PEHeaders.MetadataStartOffset + md.GetTableMetadataOffset(TableIndex.MethodDef);
                int size = md.GetTableRowSize(TableIndex.MethodDef);
                int rows = md.GetTableRowCount(TableIndex.MethodDef);
                for (int i = 0; i < rows; i++)
                {
                    int row = at + i * size;
                    uint rva = BinaryPrimitives.ReadUInt32LittleEndian(bytes.AsSpan(row, 4));
                    ushort flags = BinaryPrimitives.ReadUInt16LittleEndian(bytes.AsSpan(row + 4, 2));
                    MethodDefinition def = md.GetMethodDefinition(MetadataTokens.MethodDefinitionHandle(i + 1));
                    if ((uint)def.RelativeVirtualAddress != rva || (ushort)def.ImplAttributes != flags)
                    {
                        Log.LogError("REFUSE: the program's own code was not marked: row {0} of the method table of {1} does not read back at its place in the file", i + 1, source);
                        return false;
                    }
                    if (rva == 0 || (flags & CodeTypeMask) != 0)
                    {
                        continue;
                    }
                    withIl++;
                    if ((flags & AggressiveOptimization) != 0)
                    {
                        kept++;
                        continue;
                    }
                    BinaryPrimitives.WriteUInt16LittleEndian(bytes.AsSpan(row + 4, 2), (ushort)(flags | MarkBits));
                }
                return true;
            }
            catch (Exception e) when (e is BadImageFormatException || e is InvalidOperationException || e is ArgumentOutOfRangeException)
            {
                Log.LogError("REFUSE: the program's own code was not marked: {0} is not a .NET assembly this build can read ({1})", source, e.Message);
                return false;
            }
        }

        /// <summary>Writes the bytes unless the file already holds them.</summary>
        private static void WriteWhenDifferent(string path, byte[] bytes)
        {
            if (File.Exists(path))
            {
                byte[] held = File.ReadAllBytes(path);
                if (held.AsSpan().SequenceEqual(bytes))
                {
                    return;
                }
            }
            File.WriteAllBytes(path, bytes);
        }
    }
}
