// The board's facts generated at publish into the friend assembly of the board class (Lamella.Hardware.Facts, compiled by the targets from this one source): the row's facts file is read behind one seam and one C# source is written, byte for byte the same on every machine.

using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Text;
using System.Text.Json;
using Microsoft.Build.Framework;
using Microsoft.Build.Utilities;

namespace Lamella.Rhu.Build
{
    public sealed class BoardFactsSource : Task
    {
        [Required]
        public string FactsFile { get; set; } = string.Empty;

        [Required]
        public string Board { get; set; } = string.Empty;

        [Required]
        public string Output { get; set; } = string.Empty;

        /// <summary>The facts as the file states them: the pin table's rules, the buses, the PWM lines, the analog converters, the clock and the posture.</summary>
        private sealed class Facts
        {
            public string Name = string.Empty, Maker = string.Empty, Product = string.Empty, Chip = string.Empty, PostureName = string.Empty, Labels = string.Empty, Functions = string.Empty;
            public long ClockFrequency;
            public int Pins;
            public readonly List<(int Block, int Sda, int Scl, List<long> Rates)> I2c = new List<(int, int, int, List<long>)>();
            public readonly List<(int Block, int Sck, int Mosi, int Miso, List<int> Cs, long Min, long Max)> Spi = new List<(int, int, int, int, List<int>, long, long)>();
            public int PwmChip;
            public readonly HashSet<int> PwmLines = new HashSet<int>();
            public readonly List<(int Number, int Bits, int ReferenceMillivolts, List<(int Number, int? Pin)> Channels)> Adc = new List<(int, int, int, List<(int, int?)>)>();
        }

        /// <summary>
        /// THE READER SEAM: one flat JSON object (strings, whole numbers, arrays, objects) read
        /// with the framework's System.Text.Json; the keys as the SDK's boards/<row>.json files
        /// carry them. Another format replaces this method alone.
        /// </summary>
        private static Facts ReadFacts(string path)
        {
            var options = new JsonDocumentOptions { AllowTrailingCommas = false, CommentHandling = JsonCommentHandling.Disallow };
            using (JsonDocument doc = JsonDocument.Parse(File.ReadAllBytes(path), options))
            {
                JsonElement root = doc.RootElement;
                if (root.ValueKind != JsonValueKind.Object)
                {
                    throw new FormatException("the facts are not one JSON object");
                }

                var f = new Facts
                {
                    Name = Str(root, "name"),
                    Maker = Str(root, "maker"),
                    Product = Str(root, "product"),
                    Chip = Str(root, "chip"),
                    ClockFrequency = Num(root, "clockHz"),
                    PostureName = Str(root, "postureName"),
                    Pins = checked((int)Num(root, "pins")),
                    Labels = Str(root, "labels"),
                    Functions = Str(root, "functions"),
                };
                foreach (JsonElement b in Arr(root, "i2c").EnumerateArray())
                {
                    var rates = new List<long>();
                    foreach (JsonElement r in Arr(b, "rates").EnumerateArray())
                    {
                        rates.Add(Whole(r, "rates"));
                    }

                    f.I2c.Add((checked((int)Num(b, "block")), checked((int)Num(b, "sda")), checked((int)Num(b, "scl")), rates));
                }

                foreach (JsonElement b in Arr(root, "spi").EnumerateArray())
                {
                    var cs = new List<int>();
                    foreach (JsonElement c in Arr(b, "cs").EnumerateArray())
                    {
                        cs.Add(checked((int)Whole(c, "cs")));
                    }

                    f.Spi.Add((checked((int)Num(b, "block")), checked((int)Num(b, "sck")), checked((int)Num(b, "mosi")), checked((int)Num(b, "miso")), cs, Num(b, "min"), Num(b, "max")));
                }

                JsonElement pwm = Member(root, "pwm");
                f.PwmChip = checked((int)Num(pwm, "chip"));
                foreach (JsonElement l in Arr(pwm, "lines").EnumerateArray())
                {
                    f.PwmLines.Add(checked((int)Whole(l, "lines")));
                }

                JsonElement adc = Member(root, "adc");
                foreach (JsonElement c in Arr(adc, "converters").EnumerateArray())
                {
                    var channels = new List<(int, int?)>();
                    foreach (JsonElement ch in Arr(c, "channels").EnumerateArray())
                    {
                        JsonElement pin = Member(ch, "pin");
                        channels.Add((checked((int)Num(ch, "number")), pin.ValueKind == JsonValueKind.Null ? (int?)null : checked((int)Whole(pin, "pin"))));
                    }

                    f.Adc.Add((checked((int)Num(c, "number")), checked((int)Num(c, "bits")), checked((int)Num(c, "referenceMillivolts")), channels));
                }

                return f;
            }
        }

        private static JsonElement Member(JsonElement e, string key)
        {
            if (e.ValueKind != JsonValueKind.Object || !e.TryGetProperty(key, out JsonElement v))
            {
                throw new FormatException("no '" + key + "' member");
            }

            return v;
        }

        private static string Str(JsonElement e, string key)
        {
            JsonElement v = Member(e, key);
            return v.ValueKind == JsonValueKind.String ? v.GetString() ?? string.Empty : throw new FormatException("'" + key + "' is not a string");
        }

        private static long Whole(JsonElement v, string key)
        {
            if (v.ValueKind != JsonValueKind.Number || !v.TryGetInt64(out long n) || n < 0)
            {
                throw new FormatException("'" + key + "' is not a whole number");
            }

            return n;
        }

        private static long Num(JsonElement e, string key) => Whole(Member(e, key), key);

        private static JsonElement Arr(JsonElement e, string key)
        {
            JsonElement v = Member(e, key);
            return v.ValueKind == JsonValueKind.Array ? v : throw new FormatException("'" + key + "' is not an array");
        }

        private static (int Functions, int? I2c, int? Spi) FunctionsOf(string rule, int pin)
        {
            const int Gpio = 1, I2cSda = 2, I2cScl = 4, SpiSck = 8, SpiMosi = 16, SpiMiso = 32, SpiCs = 64, UartTx = 128, UartRx = 256, Pwm = 512;
            if (rule != "rp2350a" && rule != "rp2350b")
            {
                return (Gpio, null, null);
            }

            int f = Gpio | Pwm;
            f |= (pin % 2 == 0) ? I2cSda : I2cScl;
            int role = pin % 4;
            f |= role == 0 ? SpiMiso : role == 1 ? SpiCs : role == 2 ? SpiSck : SpiMosi;
            if (role == 0)
            {
                f |= UartTx;
            }
            else if (role == 1)
            {
                f |= UartRx;
            }


            return (f, (pin / 2) % 2, (pin / 8) % 2);
        }

        private const int PwmFunction = 512;
        private const int AdcFunction = 1024;

        private static string Quote(string s)
        {
            var b = new StringBuilder("\"");
            foreach (char c in s)
            {
                if (c == '"' || c == '\\')
                {
                    b.Append('\\');
                }

                b.Append(c);
            }

            return b.Append('"').ToString();
        }

        private static string N(long v) => v.ToString(CultureInfo.InvariantCulture);

        public override bool Execute()
        {
            try
            {
                if (!File.Exists(FactsFile))
                {
                    Log.LogError("REFUSE: no board facts for {0} ({1})", Board, FactsFile);
                    return false;
                }

                Facts f;
                try
                {
                    f = ReadFacts(FactsFile);
                }
                catch (Exception e)
                {
                    Log.LogError("REFUSE: the board facts for {0} could not be read ({1}): {2}", Board, FactsFile, e.Message);
                    return false;
                }

                if (f.Name != Board)
                {
                    Log.LogError("REFUSE: the board facts at {0} name '{1}', not the board '{2}'", FactsFile, f.Name, Board);
                    return false;
                }

                var b = new StringBuilder();
                b.Append("// <auto-generated/> Lamella RHU: the board's facts for ").Append(Board).Append(", generated at publish from the board package's boards/").Append(Board).Append(".json into the board class's friend assembly; not a file to edit.\n");
                b.Append("namespace Lamella.Hardware.Generated\n{\n    internal static class TargetBoardFacts\n    {\n");
                b.Append("        [global::System.Runtime.CompilerServices.ModuleInitializer]\n        internal static void Register()\n        {\n");
                b.Append("            var pins = new global::Lamella.Hardware.BoardPin[").Append(N(f.Pins)).Append("];\n");
                var adcOf = new Dictionary<int, int>();
                foreach (var converter in f.Adc)
                {
                    if (converter.Number != 0)
                    {
                        continue;
                    }

                    foreach (var (number, pinOfChannel) in converter.Channels)
                    {
                        if (pinOfChannel.HasValue)
                        {
                            adcOf[pinOfChannel.Value] = number;
                        }
                    }
                }

                for (int pin = 0; pin < f.Pins; pin++)
                {
                    string? label = f.Labels == "gp" ? "GP" + N(pin) : f.Labels == "port" ? "P" + (char)('A' + pin / 16) + N(pin % 16) : null;
                    var (fn, i2cBus, spiBus) = FunctionsOf(f.Functions, pin);
                    bool pwm = f.PwmLines.Contains(pin);
                    fn = pwm ? (fn | PwmFunction) : (fn & ~PwmFunction);
                    int? adcChannel = adcOf.TryGetValue(pin, out int adcNumber) ? adcNumber : (int?)null;
                    fn = adcChannel.HasValue ? (fn | AdcFunction) : (fn & ~AdcFunction);
                    b.Append("            pins[").Append(N(pin)).Append("] = new global::Lamella.Hardware.BoardPin(").Append(N(pin)).Append(", ");
                    b.Append(label is null ? "global::System.Array.Empty<string>()" : "new string[] { " + Quote(label) + " }");
                    b.Append(", (global::Lamella.Hardware.PinFunctions)").Append(N(fn)).Append(", ");
                    b.Append(i2cBus.HasValue ? N(i2cBus.Value) : "null").Append(", ");
                    b.Append(spiBus.HasValue ? N(spiBus.Value) : "null").Append(", ");
                    b.Append(pwm ? N(f.PwmChip) : "null").Append(", ").Append(pwm ? N(pin) : "null").Append(", ").Append(adcChannel.HasValue ? N(adcChannel.Value) : "null").Append(");\n");
                }

                b.Append("            var i2c = new global::Lamella.Hardware.I2cBusInfo[").Append(N(f.I2c.Count)).Append("];\n");
                for (int i = 0; i < f.I2c.Count; i++)
                {
                    var (block, sda, scl, rates) = f.I2c[i];
                    var rs = new List<string>();
                    foreach (long r in rates)
                    {
                        rs.Add(N(r));
                    }

                    b.Append("            i2c[").Append(N(i)).Append("] = new global::Lamella.Hardware.I2cBusInfo(").Append(N(block)).Append(", pins[").Append(N(sda)).Append("], pins[").Append(N(scl)).Append("], new int[] { ").Append(string.Join(", ", rs)).Append(" });\n");
                }

                b.Append("            var spi = new global::Lamella.Hardware.SpiBusInfo[").Append(N(f.Spi.Count)).Append("];\n");
                for (int i = 0; i < f.Spi.Count; i++)
                {
                    var (block, sck, mosi, miso, cs, min, max) = f.Spi[i];
                    var css = new List<string>();
                    foreach (int c in cs)
                    {
                        css.Add("pins[" + N(c) + "]");
                    }

                    b.Append("            spi[").Append(N(i)).Append("] = new global::Lamella.Hardware.SpiBusInfo(").Append(N(block)).Append(", pins[").Append(N(sck)).Append("], pins[").Append(N(mosi)).Append("], pins[").Append(N(miso)).Append("], new global::Lamella.Hardware.BoardPin[] { ").Append(string.Join(", ", css)).Append(" }, ").Append(N(min)).Append(", ").Append(N(max)).Append(");\n");
                }

                int adcBits = 0, adcMillivolts = 0;
                foreach (var (number, bits, millivolts, channels) in f.Adc)
                {
                    if (number == 0)
                    {
                        adcBits = bits;
                        adcMillivolts = millivolts;
                    }
                }

                b.Append("            global::Lamella.Hardware.TargetBoard.SetBuildFacts(new global::Lamella.Hardware.BoardFacts(");
                b.Append(Quote(f.Name)).Append(", ").Append(Quote(f.Maker)).Append(", ").Append(Quote(f.Product)).Append(", ").Append(Quote(f.Chip)).Append(", ");
                b.Append(N(f.ClockFrequency)).Append(", ").Append(Quote(f.PostureName)).Append(", pins, i2c, spi, ").Append(N(adcBits)).Append(", ").Append(N(adcMillivolts)).Append("));\n");
                b.Append("        }\n    }\n}\n");

                string dir = Path.GetDirectoryName(Path.GetFullPath(Output)) ?? ".";
                Directory.CreateDirectory(dir);
                byte[] bytes = new UTF8Encoding(false).GetBytes(b.ToString());
                if (!File.Exists(Output) || !BytesEqual(File.ReadAllBytes(Output), bytes))
                {
                    File.WriteAllBytes(Output, bytes);
                }

                return true;
            }
            catch (Exception e)
            {
                Log.LogError("REFUSE: the board facts for {0} could not be generated: {1}", Board, e.Message);
                return false;
            }
        }

        private static bool BytesEqual(byte[] a, byte[] b)
        {
            if (a.Length != b.Length)
            {
                return false;
            }

            for (int i = 0; i < a.Length; i++)
            {
                if (a[i] != b[i])
                {
                    return false;
                }
            }

            return true;
        }
    }
}
