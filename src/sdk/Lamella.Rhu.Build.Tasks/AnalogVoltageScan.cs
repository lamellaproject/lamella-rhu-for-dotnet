// The analog voltage switch's scan: whether a program's own code references one of the four voltage members of the
// standard analog classes. The compiled assemblies named (the program's own and its project references') are read
// through the framework's metadata reader: every member reference whose parent is a type of the namespace
// System.Device.Analog and whose name is ReadVoltage, or an accessor of MinVoltage, MaxVoltage or VoltageReference,
// is a hit, and the first hit is the witness (the member and the assembly that names it). A package the program takes
// is not read: the scan answers for the program's code, the assemblies a trimmer root descriptor would name.
using System;
using System.IO;
using System.Reflection.Metadata;
using System.Reflection.PortableExecutable;
using Microsoft.Build.Framework;
using Microsoft.Build.Utilities;

namespace Lamella.Rhu.Build
{
    public sealed class AnalogVoltageScan : Task
    {
        [Required]
        public ITaskItem[] Assemblies { get; set; } = Array.Empty<ITaskItem>();

        /// <summary>True when one of the assemblies references a voltage member.</summary>
        [Output]
        public bool Found { get; set; }

        /// <summary>The first hit, as "ReadVoltage referenced by program"; empty when none.</summary>
        [Output]
        public string Witness { get; set; } = string.Empty;

        private const string Namespace = "System.Device.Analog";

        private static readonly string[] Members =
        {
            "ReadVoltage", "get_MinVoltage", "get_MaxVoltage", "get_VoltageReference", "set_VoltageReference",
        };

        public override bool Execute()
        {
            Found = false;
            Witness = string.Empty;
            foreach (ITaskItem item in Assemblies)
            {
                string path = item.ItemSpec;
                if (!File.Exists(path))
                {
                    Log.LogError("REFUSE: the analog voltage scan finds no assembly at " + path);
                    return false;
                }

                string? hit;
                try
                {
                    hit = Scan(path);
                }
                catch (Exception e) when (e is BadImageFormatException || e is InvalidOperationException || e is IOException)
                {
                    Log.LogError("REFUSE: the analog voltage scan cannot read " + path + ": " + e.Message);
                    return false;
                }

                if (hit != null)
                {
                    Found = true;
                    Witness = hit + " referenced by " + Path.GetFileNameWithoutExtension(path);
                    return true;
                }
            }

            return true;
        }

        /// <summary>The first voltage member the assembly references, by the member's property or method name; null when none.</summary>
        private static string? Scan(string path)
        {
            using (FileStream stream = File.OpenRead(path))
            using (var pe = new PEReader(stream))
            {
                if (!pe.HasMetadata)
                {
                    return null;
                }

                MetadataReader md = pe.GetMetadataReader();
                foreach (MemberReferenceHandle handle in md.MemberReferences)
                {
                    MemberReference member = md.GetMemberReference(handle);
                    string name = md.GetString(member.Name);
                    if (Array.IndexOf(Members, name) < 0 || member.Parent.Kind != HandleKind.TypeReference)
                    {
                        continue;
                    }

                    TypeReference type = md.GetTypeReference((TypeReferenceHandle)member.Parent);
                    if (md.GetString(type.Namespace) != Namespace)
                    {
                        continue;
                    }

                    return name.StartsWith("get_", StringComparison.Ordinal) || name.StartsWith("set_", StringComparison.Ordinal) ? name.Substring(4) : name;
                }
            }

            return null;
        }
    }
}
