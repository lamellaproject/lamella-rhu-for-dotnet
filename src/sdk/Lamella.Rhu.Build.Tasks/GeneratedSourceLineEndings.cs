// The generated sources' line endings: every compiled source MSBuild generated under the intermediate directory (the assembly's attribute source, the framework attribute source and the global usings source) rewritten from CRLF to LF before the compile, so that a program's symbol file, its assembly and its image are the same bytes on a Windows host as on a Linux one; a file that carries no CRLF is left as it is, its time untouched.
using System;
using System.Collections.Generic;
using System.IO;
using Microsoft.Build.Framework;
using Microsoft.Build.Utilities;

namespace Lamella.Rhu.Build
{
    public sealed class GeneratedSourceLineEndings : Task
    {
        /// <summary>The compiled sources; those under the directory are read, the rest are not opened.</summary>
        [Required]
        public ITaskItem[] Sources { get; set; } = Array.Empty<ITaskItem>();

        /// <summary>The intermediate directory, a full path.</summary>
        [Required]
        public string Directory { get; set; } = string.Empty;

        /// <summary>The count of files rewritten.</summary>
        [Output]
        public int Rewritten { get; set; }

        public override bool Execute()
        {
            try
            {
                string dir = Path.GetFullPath(Directory);
                if (!dir.EndsWith(Path.DirectorySeparatorChar) && !dir.EndsWith(Path.AltDirectorySeparatorChar))
                {
                    dir += Path.DirectorySeparatorChar;
                }
                StringComparison comparison = OperatingSystem.IsWindows() ? StringComparison.OrdinalIgnoreCase : StringComparison.Ordinal;
                var seen = new HashSet<string>(OperatingSystem.IsWindows() ? StringComparer.OrdinalIgnoreCase : StringComparer.Ordinal);
                foreach (ITaskItem item in Sources)
                {
                    string full = Path.GetFullPath(item.GetMetadata("FullPath"));
                    if (!full.StartsWith(dir, comparison) || !seen.Add(full) || !File.Exists(full))
                    {
                        continue;
                    }
                    byte[] text = File.ReadAllBytes(full);
                    byte[] lf = WithoutCarriageReturns(text, out int endings);
                    if (endings == 0)
                    {
                        continue;
                    }
                    File.WriteAllBytes(full, lf);
                    Rewritten++;
                    Log.LogMessage(MessageImportance.Low, "Lamella RHU: {0} rewritten to LF ({1} line endings)", full, endings);
                }
                return true;
            }
            catch (Exception e) when (e is IOException || e is UnauthorizedAccessException || e is ArgumentException || e is NotSupportedException)
            {
                Log.LogError("REFUSE: the generated sources' line endings were not rewritten: {0}", e.Message);
                return false;
            }
        }

        /// <summary>The bytes with every CRLF pair reduced to LF; a carriage return on its own is kept.</summary>
        private static byte[] WithoutCarriageReturns(byte[] text, out int endings)
        {
            endings = 0;
            var result = new List<byte>(text.Length);
            for (int i = 0; i < text.Length; i++)
            {
                if (text[i] == 13 && i + 1 < text.Length && text[i + 1] == 10)
                {
                    endings++;
                    continue;
                }
                result.Add(text[i]);
            }
            return result.ToArray();
        }
    }
}
