// The native compiler's response file for a build that leaves the program's own code unoptimized: the compiler pack's response file read, its one line that sets the folding of identical method bodies set to the value asked, and the result written where the native compiler reads it, only when its bytes moved, so an unchanged build does not compile again. The pack sets that folding to all whenever stack traces are off, which merges two methods of one body in a program's own code into one function; the value asked here keeps each a function of its own. A response file that does not carry exactly one such line is refused by name.
using System;
using System.IO;
using System.Text;
using Microsoft.Build.Framework;
using Microsoft.Build.Utilities;

namespace Lamella.Rhu.Build
{
    public sealed class MethodBodyFolding : Task
    {
        /// <summary>The response file the compiler pack wrote.</summary>
        [Required]
        public string Source { get; set; } = string.Empty;

        /// <summary>The response file the native compiler reads.</summary>
        [Required]
        public string Output { get; set; } = string.Empty;

        /// <summary>The folding asked: all, generic or none.</summary>
        [Required]
        public string Value { get; set; } = string.Empty;

        private const string Switch = "--methodbodyfolding:";

        public override bool Execute()
        {
            try
            {
                if (Value != "all" && Value != "generic" && Value != "none")
                {
                    Log.LogError("REFUSE: the native compiler's response file was not written: the folding '{0}' is not one of all, generic, none", Value);
                    return false;
                }
                if (!File.Exists(Source))
                {
                    Log.LogError("REFUSE: the native compiler's response file was not written: the compiler pack left no response file at {0}", Source);
                    return false;
                }
                string[] lines = File.ReadAllText(Source).Split('\n');
                int found = 0;
                for (int i = 0; i < lines.Length; i++)
                {
                    bool carriageReturn = lines[i].EndsWith('\r');
                    string line = carriageReturn ? lines[i].Substring(0, lines[i].Length - 1) : lines[i];
                    if (line.StartsWith(Switch, StringComparison.Ordinal))
                    {
                        found++;
                        lines[i] = Switch + Value + (carriageReturn ? "\r" : string.Empty);
                    }
                }
                if (found != 1)
                {
                    Log.LogError("REFUSE: the native compiler's response file was not written: {0} carries {1} lines that set the folding of identical method bodies, where one is expected", Source, found);
                    return false;
                }
                byte[] bytes = new UTF8Encoding(false).GetBytes(string.Join("\n", lines));
                if (File.Exists(Output) && File.ReadAllBytes(Output).AsSpan().SequenceEqual(bytes))
                {
                    return true;
                }
                File.WriteAllBytes(Output, bytes);
                return true;
            }
            catch (Exception e) when (e is IOException || e is UnauthorizedAccessException || e is ArgumentException || e is NotSupportedException)
            {
                Log.LogError("REFUSE: the native compiler's response file was not written: {0}", e.Message);
                return false;
            }
        }
    }
}
