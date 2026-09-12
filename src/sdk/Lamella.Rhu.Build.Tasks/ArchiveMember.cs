// One member printed out of a GNU-format archive, byte for byte: the runtime archive's startup member for the merge.
using System;
using System.Collections.Generic;
using System.IO;
using System.Text;
using Microsoft.Build.Framework;
using Microsoft.Build.Utilities;

namespace Lamella.Rhu.Build
{
    public sealed class ArchiveMember : Task
    {
        [Required]
        public string Archive { get; set; } = string.Empty;

        [Required]
        public string Member { get; set; } = string.Empty;

        [Required]
        public string Output { get; set; } = string.Empty;

        public override bool Execute()
        {
            try
            {
                byte[] data = File.ReadAllBytes(Archive);
                var found = new List<(int Offset, int Size)>();
                foreach (var (name, offset, size) in Members(data))
                {
                    if (name == Member)
                    {
                        found.Add((offset, size));
                    }
                }
                if (found.Count != 1)
                {
                    Log.LogError("REFUSE: the archive {0} carries {1} member(s) named {2}, one expected", Archive, found.Count, Member);
                    return false;
                }
                var (memberOffset, memberSize) = found[0];
                if (memberSize == 0)
                {
                    Log.LogError("REFUSE: the member {0} of {1} is empty", Member, Archive);
                    return false;
                }
                if (File.Exists(Output))
                {
                    File.Delete(Output);
                }
                using (var stream = new FileStream(Output, FileMode.CreateNew, FileAccess.Write))
                {
                    stream.Write(data, memberOffset, memberSize);
                }
                Log.LogMessage(MessageImportance.High, "Lamella RHU: {0} ({1} bytes) printed out of {2}", Member, memberSize, Archive);
                return true;
            }
            catch (Exception e)
            {
                Log.LogError("REFUSE: the member {0} could not be printed out of {1}: {2}", Member, Archive, e.Message);
                return false;
            }
        }

        private static IEnumerable<(string Name, int Offset, int Size)> Members(byte[] data)
        {
            if (data.Length < 8 || Encoding.ASCII.GetString(data, 0, 8) != "!<arch>\n")
            {
                throw new InvalidOperationException("not an ar archive");
            }
            byte[] longNames = Array.Empty<byte>();
            int at = 8;
            while (at + 60 <= data.Length)
            {
                if (data[at + 58] != (byte)'`' || data[at + 59] != (byte)'\n')
                {
                    throw new InvalidOperationException("member header malformed at offset " + at);
                }
                string field = Encoding.ASCII.GetString(data, at, 16).TrimEnd(' ');
                int size = int.Parse(Encoding.ASCII.GetString(data, at + 48, 10).Trim());
                int offset = at + 60;
                if (offset + size > data.Length)
                {
                    throw new InvalidOperationException("member outside the file at offset " + at);
                }
                if (field == "//")
                {
                    longNames = new byte[size];
                    Array.Copy(data, offset, longNames, 0, size);
                }
                else if (field != "/")
                {
                    string name;
                    if (field.StartsWith("#1/", StringComparison.Ordinal))
                    {
                        throw new InvalidOperationException("a BSD-style member name at offset " + at);
                    }
                    if (field.StartsWith("/", StringComparison.Ordinal))
                    {
                        int nameOffset = int.Parse(field.Substring(1));
                        int end = nameOffset;
                        while (end < longNames.Length && longNames[end] != (byte)'/')
                        {
                            end++;
                        }
                        name = Encoding.ASCII.GetString(longNames, nameOffset, end - nameOffset);
                    }
                    else
                    {
                        name = field.EndsWith("/", StringComparison.Ordinal) ? field.Substring(0, field.Length - 1) : field;
                    }
                    yield return (name, offset, size);
                }
                at = offset + size + (size & 1);
            }
        }
    }
}
