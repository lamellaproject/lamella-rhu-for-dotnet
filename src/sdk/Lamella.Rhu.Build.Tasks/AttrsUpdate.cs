// The build-attributes update of a merged object: the .ARM.attributes section's contents replaced by the donor blob, appended at the object's end four-byte aligned with the section header re-pointed, every other byte untouched.
using System;
using System.Collections.Generic;
using System.IO;
using Microsoft.Build.Framework;
using Microsoft.Build.Utilities;

namespace Lamella.Rhu.Build
{
    public sealed class AttrsUpdate : Task
    {
        [Required]
        public string Input { get; set; } = string.Empty;

        [Required]
        public string Donor { get; set; } = string.Empty;

        [Required]
        public string Output { get; set; } = string.Empty;

        public override bool Execute()
        {
            try
            {
                byte[] pre = File.ReadAllBytes(Input);
                byte[] donor = File.ReadAllBytes(Donor);
                if (donor.Length == 0)
                {
                    Log.LogError("REFUSE: the attributes donor {0} is empty", Donor);
                    return false;
                }
                var (header, _, _) = Elf32.FindArmAttributes(pre);
                var bytes = new List<byte>(pre.Length + 4 + donor.Length);
                bytes.AddRange(pre);
                while (bytes.Count % 4 != 0)
                {
                    bytes.Add(0);
                }
                uint offset = (uint)bytes.Count;
                bytes.AddRange(donor);
                byte[] result = bytes.ToArray();
                Elf32.Write32(result, header + 16, offset);
                Elf32.Write32(result, header + 20, (uint)donor.Length);
                if (File.Exists(Output))
                {
                    File.Delete(Output);
                }
                File.WriteAllBytes(Output, result);
                Log.LogMessage(MessageImportance.High, "Lamella RHU: {0} written from {1} with the {2}-byte attributes donor", Output, Input, donor.Length);
                return true;
            }
            catch (Exception e)
            {
                Log.LogError("REFUSE: the attributes update of {0} failed: {1}", Input, e.Message);
                return false;
            }
        }
    }
}
