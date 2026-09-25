// The hosted-runtime suite: numbered sections printing PASS, the verdict line last.
using System;
using System.Collections.Generic;
using System.Device.Gpio;
using System.Device.I2c;
using System.Device.Spi;
using System.Globalization;
using System.IO;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;

namespace Rhu.Hosted;

internal static class Program
{
    private static int s_failures;

    private static void Check(bool condition, string section, string what)
    {
        if (!condition)
        {
            s_failures++;
            Console.WriteLine("FAIL " + section + " (" + what + ")");
        }
    }

    private static void Pass(string section, string note)
    {
        Console.WriteLine("PASS " + section + " " + note);
    }

    private static void Section1Smoke()
    {
        const string section = "rt-smoke";
        int before = s_failures;
        int[] values = { 3, 1, 2 };
        Array.Sort(values);
        Check(values[0] == 1 && values[1] == 2 && values[2] == 3, section, "sort order");
        string joined = string.Join(",", values);
        Check(joined == "1,2,3", section, "string join");
        if (s_failures == before)
        {
            Pass(section, "(array sort and string join)");
        }
    }

    private static void Section2Collections()
    {
        const string section = "rt-collections";
        int before = s_failures;
        Dictionary<string, int> map = new Dictionary<string, int>();
        for (int i = 0; i < 64; i++)
        {
            map["k" + i] = i * i;
        }
        Check(map.Count == 64, section, "hash map count " + map.Count);
        Check(map["k9"] == 81, section, "hash map lookup");
        Check(!map.ContainsKey("k64"), section, "absent key");
        List<int> list = new List<int>();
        for (int i = 0; i < 128; i++)
        {
            list.Add(i);
        }
        list.RemoveAll(delegate (int v) { return (v & 1) == 0; });
        Check(list.Count == 64, section, "list predicate removal " + list.Count);
        Check(list[0] == 1 && list[63] == 127, section, "list survivors");
        if (s_failures == before)
        {
            Pass(section, "(hash map of 64, list of 128 filtered to 64)");
        }
    }

    private static void Section3Text()
    {
        const string section = "rt-text";
        int before = s_failures;
        StringBuilder sb = new StringBuilder();
        for (int i = 0; i < 32; i++)
        {
            sb.Append(i.ToString(CultureInfo.InvariantCulture));
            sb.Append(';');
        }
        string s = sb.ToString();
        Check(s.Length == 86, section, "builder length " + s.Length);
        Check(s.StartsWith("0;1;2;", StringComparison.Ordinal), section, "builder prefix");
        Check(s.EndsWith("30;31;", StringComparison.Ordinal), section, "builder suffix");
        Check(255.ToString("X2", CultureInfo.InvariantCulture) == "FF", section, "hex format");
        Check((-1.5).ToString("F2", CultureInfo.InvariantCulture) == "-1.50", section, "fixed format");
        Check(int.Parse("4660", CultureInfo.InvariantCulture) == 4660, section, "integer parse");
        Check("  trim  ".Trim() == "trim", section, "trim");
        Check(string.Compare("a", "b", StringComparison.Ordinal) < 0, section, "ordinal compare");
        if (s_failures == before)
        {
            Pass(section, "(builder of 86, formats, parse, trim, compare)");
        }
    }

    private static void Section4Exceptions()
    {
        const string section = "rt-exceptions";
        int before = s_failures;
        int stage = 0;
        try
        {
            try
            {
                throw new InvalidOperationException("thrown on purpose");
            }
            finally
            {
                stage = 1;
            }
        }
        catch (InvalidOperationException)
        {
            stage = stage == 1 ? 2 : -1;
        }
        Check(stage == 2, section, "finally ran before the catch, stage " + stage);
        int divided = 0;
        try
        {
            int zero = 0;
            divided = 10 / zero;
        }
        catch (DivideByZeroException)
        {
            divided = -1;
        }
        Check(divided == -1, section, "integer divide by zero as a managed exception");
        object boxed = "not an integer";
        int cast = 0;
        try
        {
            cast = (int)boxed;
        }
        catch (InvalidCastException)
        {
            cast = -1;
        }
        Check(cast == -1, section, "invalid cast");
        int[] small = new int[2];
        int indexed = 0;
        try
        {
            indexed = small[5];
        }
        catch (IndexOutOfRangeException)
        {
            indexed = -1;
        }
        Check(indexed == -1, section, "array bound");
        if (s_failures == before)
        {
            Pass(section, "(nested finally, divide by zero, invalid cast, array bound)");
        }
    }

    private sealed class Held
    {
        public int Value;
    }

    private static void Section5Gc()
    {
        const string section = "rt-gc";
        int before = s_failures;
        Held kept = new Held { Value = 7 };
        WeakReference weak = new WeakReference(new Held { Value = 8 });
        long baseline = GC.GetTotalMemory(false);
        for (int i = 0; i < 4000; i++)
        {
            Held dead = new Held { Value = i };
            Check(dead.Value >= 0, section, "counter went negative");
        }
        int startCount = GC.CollectionCount(0);
        GC.Collect();
        GC.Collect();
        int endCount = GC.CollectionCount(0);
        Check(endCount >= startCount + 2, section, "collection count " + startCount + " to " + endCount);
        Check(kept.Value == 7, section, "strong reference survived");
        Check(!weak.IsAlive, section, "weak referent collected");
        Check(baseline > 0 && GC.GetTotalMemory(false) > 0, section, "total memory reported");
        if (s_failures == before)
        {
            Pass(section, "(4000 dead objects, two collections, weak nulled, strong kept)");
        }
    }

    private static void Section6Time()
    {
        const string section = "rt-time";
        int before = s_failures;
        long t0 = Environment.TickCount64;
        long spun = 0;
        for (int i = 0; i < 200000; i++)
        {
            spun += i & 3;
        }
        long t1 = Environment.TickCount64;
        Check(t1 >= t0, section, "tick count did not go backwards");
        Check(spun == 300000, section, "spin total " + spun);
        Check(Environment.ProcessorCount >= 1, section, "processor count " + Environment.ProcessorCount);
        if (s_failures == before)
        {
            Pass(section, "(tick count monotonic across a spin, processor count)");
        }
    }

    private interface IShape
    {
        int Area();
    }

    private sealed class Square : IShape
    {
        public int Side;
        public int Area() { return Side * Side; }
    }

    private sealed class Rect : IShape
    {
        public int W;
        public int H;
        public int Area() { return W * H; }
    }

    private static int Compare<T>(T a, T b) where T : IComparable<T>
    {
        return a.CompareTo(b);
    }

    private static void Section7Dispatch()
    {
        const string section = "rt-dispatch";
        int before = s_failures;
        IShape[] shapes = { new Square { Side = 3 }, new Rect { W = 2, H = 5 } };
        int total = 0;
        for (int i = 0; i < shapes.Length; i++)
        {
            total += shapes[i].Area();
        }
        Check(total == 19, section, "interface dispatch total " + total);
        Func<int, int> doubler = delegate (int v) { return v * 2; };
        Check(doubler(21) == 42, section, "delegate");
        Check(Compare<int>(3, 4) < 0, section, "generic comparison");
        Check(Compare<string>("b", "a") > 0, section, "generic comparison over strings");
        if (s_failures == before)
        {
            Pass(section, "(interface dispatch, delegate, generic comparison)");
        }
    }

    private static int s_shared;
    private static readonly object s_lock = new object();

    private static void Section8Threads()
    {
        const string section = "rt-threads";
        int before = s_failures;
        s_shared = 0;
        Thread t = new Thread(delegate ()
        {
            for (int i = 0; i < 5000; i++)
            {
                lock (s_lock)
                {
                    s_shared++;
                }
            }
        });
        t.Start();
        for (int i = 0; i < 5000; i++)
        {
            lock (s_lock)
            {
                s_shared++;
            }
        }
        bool joined = t.Join(30000);
        Check(joined, section, "worker joined");
        Check(s_shared == 10000, section, "shared counter " + s_shared);
        if (s_failures == before)
        {
            Pass(section, "(one worker thread, 10000 locked increments)");
        }
    }

    private static volatile int s_stop;
    private static int s_spins;

    [MethodImpl(MethodImplOptions.NoInlining)]
    private static void Tick(int n)
    {
        Volatile.Write(ref s_spins, n);
    }

    private static int WaitForProgress(int above)
    {
        for (int i = 0; i < 200000; i++)
        {
            int v = Volatile.Read(ref s_spins);
            if (v > above)
            {
                return v;
            }
            Thread.Yield();
        }
        return Volatile.Read(ref s_spins);
    }

    private static void Section9Activation()
    {
        const string section = "rt-activation";
        int before = s_failures;
        s_stop = 0;
        s_spins = 0;
        Thread t = new Thread(delegate ()
        {
            int n = 0;
            while (s_stop == 0)
            {
                n++;
                Tick(n);
            }
        });
        t.Start();
        int seen = WaitForProgress(0);
        Check(seen > 0, section, "worker running before the first collection");
        int startCount = GC.CollectionCount(0);
        for (int round = 1; round <= 3; round++)
        {
            GC.Collect();
            int after = WaitForProgress(seen);
            Check(after > seen, section, "worker resumed after collection " + round);
            seen = after;
        }
        int endCount = GC.CollectionCount(0);
        Check(endCount >= startCount + 3, section, "collection count " + startCount + " to " + endCount);
        s_stop = 1;
        bool joined = t.Join(30000);
        Check(joined, section, "worker joined");
        if (s_failures == before)
        {
            Pass(section, "(a running worker suspended and resumed across three collections)");
        }
    }

    private static readonly object s_gate = new object();
    private static volatile int s_gateReached;
    private static volatile int s_gatePassed;

    private static void Section10Parked()
    {
        const string section = "rt-parked";
        int before = s_failures;
        s_gateReached = 0;
        s_gatePassed = 0;
        Monitor.Enter(s_gate);
        Thread t = new Thread(delegate ()
        {
            s_gateReached = 1;
            lock (s_gate)
            {
                s_gatePassed = 1;
            }
        });
        t.Start();
        for (int i = 0; i < 200000 && s_gateReached == 0; i++)
        {
            Thread.Yield();
        }
        Check(s_gateReached == 1, section, "worker reached the gate");
        for (int i = 0; i < 2000; i++)
        {
            Thread.Yield();
        }
        int startCount = GC.CollectionCount(0);
        GC.Collect();
        GC.Collect();
        int endCount = GC.CollectionCount(0);
        Check(endCount >= startCount + 2, section, "collection count " + startCount + " to " + endCount);
        Check(s_gatePassed == 0, section, "worker still parked after the collections");
        Monitor.Exit(s_gate);
        bool joined = t.Join(30000);
        Check(joined, section, "worker joined");
        Check(s_gatePassed == 1, section, "worker passed the gate after its release");
        if (s_failures == before)
        {
            Pass(section, "(a worker parked on a held monitor across two collections, released and joined)");
        }
    }

    private static long s_shared64;

    private static void Section11Interlocked()
    {
        const string section = "rt-interlocked";
        int before = s_failures;
        const long first = 0x1_0000_0005L;
        const long second = 0x2_0000_0007L;
        long word = first;
        long old = Interlocked.CompareExchange(ref word, second, first);
        Check(old == first, section, "compare-exchange returned " + old);
        Check(word == second, section, "compare-exchange stored " + word);
        old = Interlocked.CompareExchange(ref word, 99, 1);
        Check(old == second && word == second, section, "a mismatched comparand left the word " + word);
        old = Interlocked.Exchange(ref word, -0x3_0000_0000L);
        Check(old == second && word == -0x3_0000_0000L, section, "exchange returned " + old + " and stored " + word);
        Check(Interlocked.Add(ref word, 0x3_0000_0001L) == 1 && word == 1, section, "add across the halves gave " + word);
        Check(Interlocked.Increment(ref word) == 2 && Interlocked.Decrement(ref word) == 1, section, "increment and decrement gave " + word);
        Check(Interlocked.Read(ref word) == 1, section, "read gave " + word);
        s_shared64 = 0;
        Thread t = new Thread(delegate ()
        {
            for (int i = 0; i < 5000; i++)
            {
                Interlocked.Increment(ref s_shared64);
            }
        });
        t.Start();
        for (int i = 0; i < 5000; i++)
        {
            Interlocked.Increment(ref s_shared64);
        }
        bool joined = t.Join(30000);
        Check(joined, section, "worker joined");
        Check(Interlocked.Read(ref s_shared64) == 10000, section, "shared 64-bit counter " + s_shared64);
        if (s_failures == before)
        {
            Pass(section, "(a long compared, exchanged, added and read above 32 bits; two threads, 10000 increments)");
        }
    }

    private static bool Near(double value, double expected)
    {
        return Math.Abs(value - expected) < 1e-9;
    }

    private static void Section12Math()
    {
        const string section = "rt-math";
        int before = s_failures;
        Check(Math.Truncate(2.75) == 2.0, section, "truncate 2.75");
        Check(Math.Truncate(-2.75) == -2.0, section, "truncate -2.75");
        Check(Math.Round(2.5, MidpointRounding.AwayFromZero) == 3.0, section, "round 2.5 away from zero");
        Check(Math.Round(-2.5, MidpointRounding.AwayFromZero) == -3.0, section, "round -2.5 away from zero");
        Check(MathF.Truncate(-3.5f) == -3.0f, section, "truncate -3.5 single");
        Check(Near(Math.Pow(2.0, 10.0), 1024.0), section, "pow");
        Check(Near(Math.Log2(1024.0), 10.0), section, "log2");
        Check(Near(Math.Cbrt(27.0), 3.0), section, "cbrt");
        Check(Near(Math.IEEERemainder(10.0, 3.0), 1.0), section, "remainder");
        if (s_failures == before)
        {
            Pass(section, "(truncate and round away from zero through the C library; pow, log2, cbrt, remainder)");
        }
    }

    private static int s_gpioEdges;
    private static readonly ManualResetEventSlim s_gpioEdge = new ManualResetEventSlim(false);
    private static GpioController? s_gpio;

    private static void OnGpioRising(object sender, PinValueChangedEventArgs e)
    {
        Interlocked.Increment(ref s_gpioEdges);
        s_gpioEdge.Set();
    }

    private static void OnGpioRisingThenLower(object sender, PinValueChangedEventArgs e)
    {
        Thread.Sleep(100);
        s_gpio!.Write(2, PinValue.Low);
    }

    private static void Section13Gpio()
    {
        const string section = "rt-gpio";
        int before = s_failures;
        Thread.Sleep(200);
        using (GpioController gpio = new GpioController())
        {
            s_gpio = gpio;
            Check(gpio.PinCount == 6, section, "pin count " + gpio.PinCount);
            Check(gpio.IsPinModeSupported(3, PinMode.InputPullUp), section, "pull-up mode supported");
            gpio.OpenPin(2, PinMode.Output, PinValue.Low);
            gpio.OpenPin(3, PinMode.Input);
            Check(gpio.Read(3) == PinValue.Low, section, "the loop reads low at open");
            gpio.Write(2, PinValue.High);
            Check(gpio.Read(3) == PinValue.High, section, "the loop reads high after a write");
            gpio.Write(2, PinValue.Low);
            Check(gpio.Read(3) == PinValue.Low, section, "the loop reads low after a second write");
            s_gpioEdges = 0;
            s_gpioEdge.Reset();
            gpio.RegisterCallbackForPinValueChangedEvent(3, PinEventTypes.Rising, OnGpioRising);
            gpio.Write(2, PinValue.High);
            Check(s_gpioEdge.Wait(2000), section, "the rising edge reached the callback within two seconds");
            Thread.Sleep(150);
            int edges = Volatile.Read(ref s_gpioEdges);
            Check(edges == 1, section, "edges counted " + edges + " for one rising edge");
            gpio.Write(2, PinValue.Low);
            Thread.Sleep(150);
            s_gpioEdge.Reset();
            GC.Collect();
            GC.Collect();
            gpio.Write(2, PinValue.High);
            Check(s_gpioEdge.Wait(2000), section, "the rising edge after two collections reached the callback within two seconds");
            Thread.Sleep(150);
            edges = Volatile.Read(ref s_gpioEdges);
            Check(edges == 2, section, "edges counted " + edges + " for two rising edges");
            gpio.UnregisterCallbackForPinValueChangedEvent(3, OnGpioRising);
            gpio.Write(2, PinValue.Low);
            gpio.RegisterCallbackForPinValueChangedEvent(3, PinEventTypes.Rising, OnGpioRisingThenLower);
            gpio.Write(2, PinValue.High);
            WaitForEventResult waited = gpio.WaitForEvent(3, PinEventTypes.Falling, CancellationToken.None);
            Check(!waited.TimedOut && waited.EventTypes == PinEventTypes.Falling, section, "the waiter saw the falling edge (timed out " + waited.TimedOut + ", kinds " + waited.EventTypes + ")");
            Check(gpio.Read(3) == PinValue.Low, section, "the loop reads low after the callback's write");
            gpio.UnregisterCallbackForPinValueChangedEvent(3, OnGpioRisingThenLower);
            bool refusedBoard = false;
            try
            {
                gpio.OpenPin(4, PinMode.Output);
            }
            catch (IOException e)
            {
                refusedBoard = e.Message.Contains("EBUSY");
            }
            Check(refusedBoard, section, "the board's line refused with EBUSY");
            bool refusedUnpopulated = false;
            try
            {
                gpio.OpenPin(5, PinMode.Input);
            }
            catch (IOException e)
            {
                refusedUnpopulated = e.Message.Contains("EINVAL");
            }
            Check(refusedUnpopulated, section, "an unpopulated line refused with EINVAL");
            gpio.ClosePin(2);
            gpio.ClosePin(3);
            s_gpio = null;
        }
        if (s_failures == before)
        {
            Pass(section, "(the standard class over the doors: six lines, an output looped to an input read back, one edge to a callback and one to a waiter, two collections during the wait, the board's and an unpopulated line refused)");
        }
    }

    [DllImport("rhu-kernel", EntryPoint = "rhu_i2c_bus")]
    private static extern int RhuI2cBus(uint index);

    [DllImport("rhu-kernel", EntryPoint = "rhu_spi_bus")]
    private static extern int RhuSpiBus(uint index);

    private const int NoSuchBus = 9;

    private static void Section14I2c()
    {
        const string section = "rt-i2c";
        int before = s_failures;
        int bus = RhuI2cBus(0);
        Check(bus >= 0, section, "the first I2C bus's number " + bus);
        using (I2cDevice sensor = I2cDevice.Create(new I2cConnectionSettings(bus, 0x76)))
        {
            byte[] id = new byte[1];
            sensor.WriteRead(new byte[] { 0xD0 }, id);
            Check(id[0] == 0x60, section, "the identity register read 0x" + id[0].ToString("X2", CultureInfo.InvariantCulture));
            byte[] burst = new byte[8];
            sensor.WriteRead(new byte[] { 0xF7 }, burst);
            Check(burst[0] == 0x80 && burst[1] == 0 && burst[2] == 0 && burst[3] == 0x80 && burst[4] == 0 && burst[5] == 0 && burst[6] == 0x80 && burst[7] == 0, section, "the eight-byte burst read " + BitConverter.ToString(burst));
            sensor.Write(new byte[] { 0xE0, 0xB6 });
            sensor.WriteByte(0xD0);
            byte again = sensor.ReadByte();
            Check(again == 0x60, section, "the identity read in a second transaction 0x" + again.ToString("X2", CultureInfo.InvariantCulture));
            bool refusedLong = false;
            try
            {
                sensor.Read(new byte[256]);
            }
            catch (IOException e)
            {
                refusedLong = e.Message.Contains("EINVAL");
            }
            Check(refusedLong, section, "a 256-byte message refused with EINVAL");
        }
        using (I2cBus shared = I2cBus.Create(bus))
        {
            using (I2cDevice second = shared.CreateDevice(0x76))
            {
                byte[] id = new byte[1];
                second.WriteRead(new byte[] { 0xD0 }, id);
                Check(id[0] == 0x60, section, "the identity through a second descriptor 0x" + id[0].ToString("X2", CultureInfo.InvariantCulture));
            }
        }
        using (I2cDevice absent = I2cDevice.Create(new I2cConnectionSettings(bus, 0x77)))
        {
            bool refusedAddress = false;
            try
            {
                absent.ReadByte();
            }
            catch (IOException e)
            {
                refusedAddress = e.Message.Contains("ENXIO");
            }
            Check(refusedAddress, section, "an unanswered address refused with ENXIO");
        }
        bool refusedBus = false;
        try
        {
            using (I2cDevice none = I2cDevice.Create(new I2cConnectionSettings(NoSuchBus, 0x76)))
            {
                none.ReadByte();
            }
        }
        catch (IOException e)
        {
            refusedBus = e.Message.Contains("ENOENT");
        }
        Check(refusedBus, section, "a missing bus refused with ENOENT");
        if (s_failures == before)
        {
            Pass(section, "(the standard class over the doors: the sensor model's identity and burst read through combined transactions, its reset written, a second descriptor on the shared bus, an unanswered address ENXIO, a message over the limit EINVAL, a missing bus ENOENT)");
        }
    }

    private static bool SpiRefused(SpiConnectionSettings settings, string errno)
    {
        try
        {
            using (SpiDevice device = SpiDevice.Create(settings))
            {
                device.ReadByte();
            }
            return false;
        }
        catch (IOException e)
        {
            return e.Message.Contains(errno);
        }
    }

    private static void Section15Spi()
    {
        const string section = "rt-spi";
        int before = s_failures;
        int bus = RhuSpiBus(0);
        Check(bus >= 0, section, "the first SPI bus's number " + bus);
        using (SpiDevice flash = SpiDevice.Create(new SpiConnectionSettings(bus, 0) { ClockFrequency = 1000000 }))
        {
            byte[] identity = new byte[4];
            flash.TransferFullDuplex(new byte[] { 0x9F, 0, 0, 0 }, identity);
            Check(identity[1] == 0xEF && identity[2] == 0x40 && identity[3] == 0x18, section, "the identity read " + BitConverter.ToString(identity));
            byte[] five = new byte[5];
            flash.TransferFullDuplex(new byte[] { 0x03, 1, 2, 3, 4 }, five);
            Check(five[1] == 1 && five[2] == 2 && five[3] == 3 && five[4] == 4, section, "the four-byte echo read " + BitConverter.ToString(five));
            byte[] big = new byte[512];
            byte[] back = new byte[512];
            big[0] = 0x03;
            for (int i = 1; i < big.Length; i++)
            {
                big[i] = (byte)i;
            }
            flash.TransferFullDuplex(big, back);
            bool intact = true;
            for (int i = 1; i < big.Length; i++)
            {
                if (back[i] != big[i])
                {
                    intact = false;
                }
            }
            Check(intact, section, "the 512-byte echo intact");
            byte[] ones = new byte[4];
            flash.Read(ones);
            Check(ones[0] == 0xFF && ones[1] == 0xFF && ones[2] == 0xFF && ones[3] == 0xFF, section, "a read shifts out ones and reads them back " + BitConverter.ToString(ones));
            flash.Write(new byte[] { 0x03, 7 });
            using (SpiDevice other = SpiDevice.Create(new SpiConnectionSettings(bus, 1) { Mode = SpiMode.Mode3, ClockFrequency = 500000 }))
            {
                byte[] three = new byte[3];
                other.TransferFullDuplex(new byte[] { 0x03, 9, 8 }, three);
                Check(three[0] == 0xFF && three[1] == 0xFF && three[2] == 0xFF, section, "the second select reads ones " + BitConverter.ToString(three));
                byte[] echo = new byte[3];
                flash.TransferFullDuplex(new byte[] { 0x03, 5, 6 }, echo);
                Check(echo[1] == 5 && echo[2] == 6, section, "the first device's echo after the second device's transfer " + BitConverter.ToString(echo));
            }
        }
        using (SpiDevice own = SpiDevice.Create(new SpiConnectionSettings(bus, -1) { ClockFrequency = 1000000 }))
        {
            byte[] three = new byte[3];
            own.Read(three);
            Check(three[0] == 0xFF && three[1] == 0xFF && three[2] == 0xFF, section, "a device driving its own select reads ones " + BitConverter.ToString(three));
        }
        using (SpiDevice reversed = SpiDevice.Create(new SpiConnectionSettings(bus, 0) { ClockFrequency = 1000000, DataFlow = DataFlow.LsbFirst }))
        {
            byte[] two = new byte[2];
            reversed.TransferFullDuplex(new byte[] { 0x03, 0x01 }, two);
            Check(two[1] == 0x01, section, "the least-significant-bit-first echo read " + BitConverter.ToString(two));
        }
        Check(SpiRefused(new SpiConnectionSettings(bus, 2) { ClockFrequency = 1000000 }, "EINVAL"), section, "a select the bus lacks refused with EINVAL");
        Check(SpiRefused(new SpiConnectionSettings(bus, 0) { ClockFrequency = 1000000, DataBitLength = 16 }, "EINVAL"), section, "a sixteen-bit word refused with EINVAL");
        Check(SpiRefused(new SpiConnectionSettings(bus, 0) { ClockFrequency = 1000000, ChipSelectLineActiveState = PinValue.High }, "EINVAL"), section, "an active-high select refused with EINVAL");
        Check(SpiRefused(new SpiConnectionSettings(NoSuchBus, 0) { ClockFrequency = 1000000 }, "ENOENT"), section, "a missing bus refused with ENOENT");
        if (s_failures == before)
        {
            Pass(section, "(the standard class over the doors: the memory model's identity at the achieved rate, echoes of four and five hundred and twelve bytes, a read shifting out ones, a second device with its own mode and rate on the shared bus, a device driving its own select, a select the bus lacks, a word size, an active-high select and a missing bus refused, least-significant-bit-first served by the class's own reversal)");
        }
    }

    private static void Guarded(string section, Action body)
    {
        try
        {
            body();
        }
        catch (Exception e)
        {
            Check(false, section, "an unexpected " + e.GetType().Name + ": " + e.Message);
        }
    }

    private static int Main()
    {
        Console.WriteLine("RTSUITE: start");
        Section1Smoke();
        Section2Collections();
        Section3Text();
        Section4Exceptions();
        Section5Gc();
        Section6Time();
        Section7Dispatch();
        Section8Threads();
        Section9Activation();
        Section10Parked();
        Section11Interlocked();
        Section12Math();
        Section13Gpio();
        Guarded("rt-i2c", Section14I2c);
        Guarded("rt-spi", Section15Spi);
        if (s_failures != 0)
        {
            Console.WriteLine("RTSUITE: FAIL (" + s_failures + " checks)");
            return 1;
        }
        Console.WriteLine("RTSUITE: ALL GOOD");
        return 0;
    }
}
