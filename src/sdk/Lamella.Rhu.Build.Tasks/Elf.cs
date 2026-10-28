// The ELF32 reader the tasks share: the one .ARM.attributes section of a little-endian ARM relocatable, and a named section of a linked image, each located by its section header.
using System;
using System.Text;

namespace Lamella.Rhu.Build
{
    internal static class Elf32
    {
        internal static (int Header, int Offset, int Size) FindArmAttributes(byte[] obj)
        {
            if (obj.Length < 52 || obj[0] != 0x7f || obj[1] != (byte)'E' || obj[2] != (byte)'L' || obj[3] != (byte)'F')
            {
                throw new InvalidOperationException("not an ELF object");
            }
            if (obj[4] != 1)
            {
                throw new InvalidOperationException("not a 32-bit ELF object");
            }
            if (obj[5] != 1)
            {
                throw new InvalidOperationException("not a little-endian ELF object");
            }
            if (Read16(obj, 16) != 1)
            {
                throw new InvalidOperationException("not a relocatable (ET_REL) object");
            }
            if (Read16(obj, 18) != 40)
            {
                throw new InvalidOperationException("not an ARM object");
            }
            int shoff = checked((int)Read32(obj, 32));
            int shentsize = Read16(obj, 46);
            int shnum = Read16(obj, 48);
            int shstrndx = Read16(obj, 50);
            if (shentsize < 40 || shnum == 0 || shstrndx >= shnum)
            {
                throw new InvalidOperationException("section header table malformed");
            }
            uint Field(int index, int field) => Read32(obj, shoff + index * shentsize + field);
            int strOff = checked((int)Field(shstrndx, 16));
            int strSize = checked((int)Field(shstrndx, 20));
            if (strOff < 0 || strSize < 0 || strOff + strSize > obj.Length)
            {
                throw new InvalidOperationException("section name table outside the file");
            }
            int found = -1;
            for (int i = 0; i < shnum; i++)
            {
                int nameOff = checked((int)Field(i, 0));
                if (NameAt(obj, strOff, strSize, nameOff) == ".ARM.attributes")
                {
                    if (Field(i, 4) != 0x70000003u)
                    {
                        throw new InvalidOperationException("the .ARM.attributes section's type is not SHT_ARM_ATTRIBUTES");
                    }
                    if (found >= 0)
                    {
                        throw new InvalidOperationException("two .ARM.attributes sections");
                    }
                    found = i;
                }
            }
            if (found < 0)
            {
                throw new InvalidOperationException("no .ARM.attributes section");
            }
            int header = shoff + found * shentsize;
            return (header, checked((int)Field(found, 16)), checked((int)Field(found, 20)));
        }

        /// <summary>
        /// The one section of the name in a little-endian ARM executable image, of the type given:
        /// its contents' offset and size, or a refusal naming the fault (no such section, two of
        /// them, another type, or contents outside the file).
        /// </summary>
        internal static (int Offset, int Size) FindSection(byte[] elf, string name, uint type)
        {
            if (elf.Length < 52 || elf[0] != 0x7f || elf[1] != (byte)'E' || elf[2] != (byte)'L' || elf[3] != (byte)'F')
            {
                throw new InvalidOperationException("the image is not an ELF file");
            }
            if (elf[4] != 1 || elf[5] != 1)
            {
                throw new InvalidOperationException("the image is not a 32-bit little-endian ELF file");
            }
            if (Read16(elf, 16) != 2)
            {
                throw new InvalidOperationException("the image is not an executable (ET_EXEC) ELF file");
            }
            if (Read16(elf, 18) != 40)
            {
                throw new InvalidOperationException("the image is not an ARM ELF file");
            }
            int shoff = checked((int)Read32(elf, 32));
            int shentsize = Read16(elf, 46);
            int shnum = Read16(elf, 48);
            int shstrndx = Read16(elf, 50);
            if (shentsize < 40 || shnum == 0 || shstrndx >= shnum)
            {
                throw new InvalidOperationException("the image's section header table is malformed");
            }
            uint Field(int index, int field) => Read32(elf, shoff + index * shentsize + field);
            int strOff = checked((int)Field(shstrndx, 16));
            int strSize = checked((int)Field(shstrndx, 20));
            if (strOff < 0 || strSize < 0 || strOff + strSize > elf.Length)
            {
                throw new InvalidOperationException("the image's section name table lies outside the file");
            }
            int found = -1;
            for (int i = 0; i < shnum; i++)
            {
                if (NameAt(elf, strOff, strSize, checked((int)Field(i, 0))) != name)
                {
                    continue;
                }
                if (Field(i, 4) != type)
                {
                    throw new InvalidOperationException(string.Format(System.Globalization.CultureInfo.InvariantCulture, "the {0} section's type is {1}, not {2}", name, Field(i, 4), type));
                }
                if (found >= 0)
                {
                    throw new InvalidOperationException("two " + name + " sections");
                }
                found = i;
            }
            if (found < 0)
            {
                throw new InvalidOperationException("no " + name + " section");
            }
            int offset = checked((int)Field(found, 16));
            int size = checked((int)Field(found, 20));
            if (offset < 0 || size < 0 || offset + size > elf.Length)
            {
                throw new InvalidOperationException("the " + name + " section lies outside the file");
            }
            return (offset, size);
        }

        internal static ushort Read16(byte[] obj, int offset)
        {
            if (offset < 0 || offset + 2 > obj.Length)
            {
                throw new InvalidOperationException("object truncated at offset " + offset);
            }
            return (ushort)(obj[offset] | (obj[offset + 1] << 8));
        }

        internal static uint Read32(byte[] obj, int offset)
        {
            if (offset < 0 || offset + 4 > obj.Length)
            {
                throw new InvalidOperationException("object truncated at offset " + offset);
            }
            return (uint)(obj[offset] | (obj[offset + 1] << 8) | (obj[offset + 2] << 16) | (obj[offset + 3] << 24));
        }

        internal static void Write32(byte[] obj, int offset, uint value)
        {
            obj[offset] = (byte)value;
            obj[offset + 1] = (byte)(value >> 8);
            obj[offset + 2] = (byte)(value >> 16);
            obj[offset + 3] = (byte)(value >> 24);
        }

        private static string NameAt(byte[] obj, int strOff, int strSize, int nameOff)
        {
            if (nameOff < 0 || nameOff >= strSize)
            {
                return string.Empty;
            }
            int start = strOff + nameOff;
            int end = start;
            while (end < strOff + strSize && obj[end] != 0)
            {
                end++;
            }
            return Encoding.ASCII.GetString(obj, start, end - start);
        }
    }
}
