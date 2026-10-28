// The hosted-runtime suite: numbered sections printing PASS, the verdict line last.

using System;
using System.Collections.Generic;
using System.Device.Analog;
using System.Device.Gpio;
using System.Device.I2c;
using System.Device.Pwm;
using System.Device.Spi;
using System.Globalization;
using System.IO;
using System.IO.Ports;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;
using Lamella.Hardware;

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

    // The GPIO section's edge count and its signal: the callback runs on
    // the package's own event thread (a thread-pool worker), so the count
    // is read and written through the interlocked family. The section
    // starts no thread of its own and takes no timer: the package's driver
    // needs two pool threads (its event loop's worker and the pool's gate
    // thread), and an emulator row's stack pool has room for those beside
    // the runtime's own thread and nothing more -- so the falling edge the
    // waiter on the main thread expects is written FROM the rising edge's
    // callback after a short sleep on the event thread, and the waits are
    // bounded by the emulator run's own timeout, never by a timer thread.
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
        // The main thread enters its wait for the falling edge within
        // microseconds of the write that raised this edge; the sleep here
        // keeps the falling edge behind that entry.
        Thread.Sleep(100);
        s_gpio!.Write(2, PinValue.Low);
    }

    private static void Section13Gpio()
    {
        const string section = "rt-gpio";
        int before = s_failures;
        // The package's driver asks the runtime's thread pool for two threads;
        // on an emulator row's stack pool they fit only once the previous
        // sections' workers have left it -- a joined worker's stack returns a
        // moment after the join, at its own exit -- so the section yields first.
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
            // One rising edge to a callback, then a settle: exactly one edge counted.
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
            // One falling edge to a waiter on the main thread: the rising edge's
            // callback lowers the line from the event thread after its sleep.
            gpio.Write(2, PinValue.Low);
            gpio.RegisterCallbackForPinValueChangedEvent(3, PinEventTypes.Rising, OnGpioRisingThenLower);
            gpio.Write(2, PinValue.High);
            WaitForEventResult waited = gpio.WaitForEvent(3, PinEventTypes.Falling, CancellationToken.None);
            Check(!waited.TimedOut && waited.EventTypes == PinEventTypes.Falling, section, "the waiter saw the falling edge (timed out " + waited.TimedOut + ", kinds " + waited.EventTypes + ")");
            Check(gpio.Read(3) == PinValue.Low, section, "the loop reads low after the callback's write");
            gpio.UnregisterCallbackForPinValueChangedEvent(3, OnGpioRisingThenLower);
            // The refusals through the class: the board's line, an unpopulated line.
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

    // The kernel's bus numbers: the chip's block numbers, as Linux names the
    // nodes (I2C1 is /dev/i2c-1), read through the kernel's own doors bound at
    // link time, so this one program serves both emulator worlds -- whose
    // models are the same I2C block but different SPI blocks.
    [DllImport("rhu-kernel", EntryPoint = "rhu_i2c_bus")]
    private static extern int RhuI2cBus(uint index);

    [DllImport("rhu-kernel", EntryPoint = "rhu_spi_bus")]
    private static extern int RhuSpiBus(uint index);

    // A bus number no row of this suite has: its node does not exist.
    private const int NoSuchBus = 9;

    private static void Section14I2c()
    {
        const string section = "rt-i2c";
        int before = s_failures;
        int bus = RhuI2cBus(0);
        Check(bus >= 0, section, "the first I2C bus's number " + bus);
        // The sensor model behind the emulator row's I2C block, at address
        // 0x76: its identity register answers 0x60, its data burst eight fixed
        // bytes, it acknowledges the reset write, and its register pointer
        // holds across a stop. The class opens the bus's node and asks the host
        // for each transaction; the host claims the bus at 100 kHz at the open.
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
            // A 256-byte read is over the kernel's message limit: refused whole.
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
        // A second descriptor on the same bus, through the bus class.
        using (I2cBus shared = I2cBus.Create(bus))
        {
            using (I2cDevice second = shared.CreateDevice(0x76))
            {
                byte[] id = new byte[1];
                second.WriteRead(new byte[] { 0xD0 }, id);
                Check(id[0] == 0x60, section, "the identity through a second descriptor 0x" + id[0].ToString("X2", CultureInfo.InvariantCulture));
            }
        }
        // An address nothing answers: the transaction refused with ENXIO.
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
        // A bus the board does not have: the open refused with ENOENT.
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
            Pass(section, "(the standard class over the doors: the sensor model's identity and burst read in combined transactions, its reset written, a second descriptor on the bus; an unanswered address ENXIO, an oversize message EINVAL, a missing bus ENOENT)");
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
        // The memory model behind the emulator row's SPI block, under the
        // bus's first select: the identity command answers EF 40 18, any other
        // command echoes each byte in its own frame; a byte clocked with no
        // select asserted, or under the second select, answers ones. The class
        // opens the node at its first transfer and sets the node's mode, word
        // size and rate, which the host carries into every transfer.
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
            // A second device with its own mode and rate on the shared bus: the
            // second select reaches no device on this row, so it reads ones; the
            // first device's settings ride its own next transfer unchanged.
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
        // A device driving its own select through the GPIO class: the node
        // transfers with no select asserted and reads ones.
        using (SpiDevice own = SpiDevice.Create(new SpiConnectionSettings(bus, -1) { ClockFrequency = 1000000 }))
        {
            byte[] three = new byte[3];
            own.Read(three);
            Check(three[0] == 0xFF && three[1] == 0xFF && three[2] == 0xFF, section, "a device driving its own select reads ones " + BitConverter.ToString(three));
        }
        // Least-significant-bit-first: the host refuses the mode bit once and
        // the class reverses the bytes itself, so the echo returns them.
        using (SpiDevice reversed = SpiDevice.Create(new SpiConnectionSettings(bus, 0) { ClockFrequency = 1000000, DataFlow = DataFlow.LsbFirst }))
        {
            byte[] two = new byte[2];
            reversed.TransferFullDuplex(new byte[] { 0x03, 0x01 }, two);
            Check(two[1] == 0x01, section, "the least-significant-bit-first echo read " + BitConverter.ToString(two));
        }
        // The refusals through the class: a select the bus lacks (at the first
        // transfer), a word size, an active-high select, a missing bus.
        Check(SpiRefused(new SpiConnectionSettings(bus, 2) { ClockFrequency = 1000000 }, "EINVAL"), section, "a select the bus lacks refused with EINVAL");
        Check(SpiRefused(new SpiConnectionSettings(bus, 0) { ClockFrequency = 1000000, DataBitLength = 16 }, "EINVAL"), section, "a sixteen-bit word refused with EINVAL");
        Check(SpiRefused(new SpiConnectionSettings(bus, 0) { ClockFrequency = 1000000, ChipSelectLineActiveState = PinValue.High }, "EINVAL"), section, "an active-high select refused with EINVAL");
        Check(SpiRefused(new SpiConnectionSettings(NoSuchBus, 0) { ClockFrequency = 1000000 }, "ENOENT"), section, "a missing bus refused with ENOENT");
        if (s_failures == before)
        {
            Pass(section, "(the standard class over the doors: the memory model's identity at the achieved rate; echoes of 4 and 512 bytes; a read shifting out ones; a second device, mode and rate; four refusals; LSB-first by the class's reversal)");
        }
    }


    // The PWM section drives the standard package's PWM class over the
    // shim's PWM device files (the Linux sysfs convention under
    // /sys/class/pwm/pwmchip0) on the kernel's PWM doors, on the emulator
    // row's two virtual timers: the lines 0 and 2 on timer 0, the line 3 on
    // timer 1, the line 1 a hole. THE CHANNEL NUMBER IS THE LINE NUMBER, and
    // the period file reads back the period the timer runs, not the value
    // asked (the one departure from Linux a program can see).
    private const string PwmChip = "/sys/class/pwm/pwmchip0";

    private static string PwmText(int channel, string file)
    {
        return File.ReadAllText(PwmChip + "/pwm" + channel + "/" + file);
    }

    private static bool PwmRefused<T>(int chip, int channel, int frequency) where T : Exception
    {
        try
        {
            using (PwmChannel refused = PwmChannel.Create(chip, channel, frequency, 0.5))
            {
            }

            return false;
        }
        catch (T)
        {
            return true;
        }
    }

    private static void Section16Pwm()
    {
        const string section = "rt-pwm";
        int before = s_failures;
        // (1) The class on channel 0 at 1 kHz and 25 percent: the files as the
        // class wrote them, then Start, a new duty, a new frequency (the class
        // re-writes the duty after the period), Stop and Dispose.
        using (PwmChannel pwm = PwmChannel.Create(0, 0, 1000, 0.25))
        {
            Check(PwmText(0, "period") == "1000000\n", section, "the period file after Create reads " + PwmText(0, "period").Trim());
            Check(PwmText(0, "duty_cycle") == "250000\n", section, "the duty file after Create reads " + PwmText(0, "duty_cycle").Trim());
            Check(PwmText(0, "polarity") == "normal\n", section, "the polarity file after Create reads " + PwmText(0, "polarity").Trim());
            Check(PwmText(0, "enable") == "0\n", section, "the enable file after Create reads " + PwmText(0, "enable").Trim());
            pwm.Start();
            Check(PwmText(0, "enable") == "1\n", section, "the enable file after Start reads " + PwmText(0, "enable").Trim());
            pwm.DutyCycle = 0.75;
            Check(PwmText(0, "duty_cycle") == "750000\n", section, "the duty file at 75 percent reads " + PwmText(0, "duty_cycle").Trim());
            pwm.Frequency = 250;
            Check(PwmText(0, "period") == "4000000\n" && PwmText(0, "duty_cycle") == "3000000\n", section, "the period and duty files at 250 Hz read " + PwmText(0, "period").Trim() + " and " + PwmText(0, "duty_cycle").Trim());
            pwm.Stop();
            Check(PwmText(0, "enable") == "0\n", section, "the enable file after Stop reads " + PwmText(0, "enable").Trim());
        }

        Check(!Directory.Exists(PwmChip + "/pwm0"), section, "the channel directory gone after Dispose");
        // (2) The quantized truth: 3 Hz asked is the class's 333333333 ns; the
        // virtual timer runs 333336000 ns, and the period file says so.
        using (PwmChannel slow = PwmChannel.Create(0, 3, 3, 0.5))
        {
            Check(PwmText(3, "period") == "333336000\n", section, "the period the timer runs read back for 3 Hz: " + PwmText(3, "period").Trim());
        }

        // (3) The refusals through the class: a chip the row lacks, a channel
        // past npwm, the hole (its export refused ENODEV on the wire).
        Check(PwmRefused<ArgumentException>(1, 0, 1000), section, "chip 1 refused with ArgumentException");
        Check(PwmRefused<ArgumentException>(0, 4, 1000), section, "channel 4 refused with ArgumentException");
        Check(PwmRefused<IOException>(0, 1, 1000), section, "the hole, channel 1, refused with IOException");
        // (4) The shared period: with channel 0 held at 1 kHz, another period
        // on channel 2 (the same timer) is refused at the period write, EBUSY
        // on the wire. The failed constructor leaves its export and two open
        // streams behind, as it would on Linux: the streams close with the
        // finalizers, the export is removed by the program, and the same
        // period then succeeds.
        using (PwmChannel held = PwmChannel.Create(0, 0, 1000, 0.5))
        {
            Check(PwmRefused<IOException>(0, 2, 2000), section, "another period on the shared timer refused with IOException");
            GC.Collect();
            GC.WaitForPendingFinalizers();
            GC.Collect();
            GC.WaitForPendingFinalizers();
            File.WriteAllText(PwmChip + "/unexport", "2");
            using (PwmChannel same = PwmChannel.Create(0, 2, 1000, 0.5))
            {
                Check(PwmText(2, "period") == "1000000\n", section, "the same period on the shared timer accepted: " + PwmText(2, "period").Trim());
            }
        }

        // (5) A line held as a GPIO output refuses the export (EBUSY on the
        // wire); closed, it is a PWM channel again, through the board class's
        // form of the standard call (the chip, the channel, the frequency, the
        // duty cycle); the board class refuses a chip the row lacks and, in the
        // form that names a pin, a pin other than the channel's line.
        using (GpioController gpio = new GpioController())
        {
            gpio.OpenPin(3, PinMode.Output);
            Check(PwmRefused<IOException>(0, 3, 1000), section, "a line held as a GPIO output refused with IOException");
            gpio.ClosePin(3);
            TargetBoard board = new TargetBoard();
            using (PwmChannel freed = board.CreatePwmChannel(0, 3, 1000, 0.5))
            {
                Check(Directory.Exists(PwmChip + "/pwm3"), section, "the line given back by the GPIO class exports as a PWM channel through the board class");
            }

            Check(Refused<NotSupportedException>(() => board.CreatePwmChannel(1, 3), "chip 1"), section, "the board class refuses chip 1 with NotSupportedException");
            Check(Refused<NotSupportedException>(() => board.CreatePwmChannel(0, 3, 1000, 0.5, 2), "pin 2"), section, "the board class refuses channel 3 on pin 2 with NotSupportedException");
        }

        // (6) Every channel disposed: line 0 opens as a GPIO line again.
        using (GpioController gpio = new GpioController())
        {
            gpio.OpenPin(0, PinMode.Input);
            Check(gpio.IsPinOpen(0), section, "line 0 opens as a GPIO line after every channel's disposal");
        }

        if (s_failures == before)
        {
            Pass(section, "(the standard class over the sysfs files on the doors: the channel number is the line number; Start, duty, frequency and Stop; the quantized period; the refusals; the shared period EBUSY; a GPIO holder refuses and gives back)");
        }
    }

    // The bus sections drive the package's drivers over the host's device
    // nodes: a request the host refuses that the section did not expect
    // surfaces as the class's exception, which is the section's failure to
    // report, not the suite's to die on (the gate's controls read the
    // section's FAIL line; a consumer reads the exception's words).
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

    private interface IValued
    {
        int Get();
    }

    private sealed class Valued : IValued
    {
        public int Value;

        public int Get()
        {
            return Value;
        }

        public int Twice()
        {
            return Value * 2;
        }
    }

    [StructLayout(LayoutKind.Explicit)]
    private sealed class Far
    {
        [FieldOffset(3000)]
        public int Field;
    }

    // Each null is opaque to the compiler: a constant null would let it throw
    // without any fault, and the point of the section is the fault.
    [MethodImpl(MethodImplOptions.NoInlining)]
    private static Valued NullValued()
    {
        return null;
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    private static int[] NullArray()
    {
        return null;
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    private static Far NullFar()
    {
        return null;
    }

    private static void Section17Null()
    {
        const string section = "rt-null";
        int before = s_failures;
        string read = "none";
        try
        {
            Valued v = NullValued();
            read = v.Value.ToString(CultureInfo.InvariantCulture);
        }
        catch (NullReferenceException)
        {
            read = "NullReferenceException";
        }
        Check(read == "NullReferenceException", section, "a field read through null: " + read);
        string write = "none";
        try
        {
            Valued v = NullValued();
            v.Value = 1;
        }
        catch (NullReferenceException)
        {
            write = "NullReferenceException";
        }
        Check(write == "NullReferenceException", section, "a field write through null: " + write);
        string element = "none";
        try
        {
            int[] a = NullArray();
            element = a[0].ToString(CultureInfo.InvariantCulture);
        }
        catch (NullReferenceException)
        {
            element = "NullReferenceException";
        }
        Check(element == "NullReferenceException", section, "an array element through null: " + element);
        string call = "none";
        try
        {
            Valued v = NullValued();
            call = v.Twice().ToString(CultureInfo.InvariantCulture);
        }
        catch (NullReferenceException)
        {
            call = "NullReferenceException";
        }
        Check(call == "NullReferenceException", section, "an instance call through null: " + call);
        string dispatch = "none";
        try
        {
            IValued i = NullValued();
            dispatch = i.Get().ToString(CultureInfo.InvariantCulture);
        }
        catch (NullReferenceException)
        {
            dispatch = "NullReferenceException";
        }
        Check(dispatch == "NullReferenceException", section, "an interface call through null: " + dispatch);
        string far = "none";
        try
        {
            Far f = NullFar();
            far = f.Field.ToString(CultureInfo.InvariantCulture);
        }
        catch (NullReferenceException)
        {
            far = "NullReferenceException";
        }
        Check(far == "NullReferenceException", section, "a field past the unchecked window through null: " + far);
        if (s_failures == before)
        {
            Pass(section, "(a null reference as the managed exception: a field read and write, an array element, an instance and an interface call each caught through the guard; a field past the unchecked window and a read at offset 0 caught the same)");
        }
    }

    // The eighteenth section drives the emulator row's second UART through
    // the standard serial port package's class, unchanged: the package's own
    // Linux driver opens the device file /dev/ttyS<n>, which the host
    // presents over the kernel's UART doors (the port's number is the chip's
    // block number), and the package's native layer is compiled unchanged
    // into the host from its own sources. The far end of the second UART is
    // the verification chain's echo, so what the section writes comes back.
    // Every wait is bounded, so an image without the echo ends with the
    // section's FAIL lines instead of hanging. The class caches the rate it
    // was given and never reads it back, so a rate outside the standard
    // codes is checked as accepted, not as read back.
    private const string SerialPortName = "/dev/ttyS1";
    private static int s_serialEvents;

    private static bool SerialRefused<T>(Action act) where T : Exception
    {
        try
        {
            act();
            return false;
        }
        catch (T)
        {
            return true;
        }
    }

    private static bool WaitFor(Func<bool> test, int boundMs)
    {
        long end = Environment.TickCount64 + boundMs;
        while (!test())
        {
            if (Environment.TickCount64 >= end)
            {
                return false;
            }
            Thread.Sleep(1);
        }
        return true;
    }

    private static int ReadWhole(SerialPort port, byte[] buffer)
    {
        int got = 0;
        while (got < buffer.Length)
        {
            int n = port.Read(buffer, got, buffer.Length - got);
            if (n <= 0)
            {
                break;
            }
            got += n;
        }
        return got;
    }

    private static void Section18Serial()
    {
        const string section = "rt-serial";
        int before = s_failures;
        // (1) The enumeration: the row's two UART blocks as /dev/ttyS<n>, in
        // the rows' order.
        string[] names = SerialPort.GetPortNames();
        Check(names.Length == 2 && names[0] == "/dev/ttyS0" && names[1] == "/dev/ttyS1", section, "the port names: " + string.Join(",", names));
        using (SerialPort port = new SerialPort(SerialPortName, 115200, Parity.None, 8, StopBits.One))
        {
            port.Open();
            Check(port.IsOpen, section, "the port open at 115200 8N1");
            // (2) The refusals through the class: a second opener of the held
            // block and a name past the row's blocks (every failed open is the
            // class's access exception, as on Windows).
            Check(SerialRefused<UnauthorizedAccessException>(() => { using (SerialPort second = new SerialPort(SerialPortName, 115200)) { second.Open(); } }), section, "a second opener refused with UnauthorizedAccessException");
            Check(SerialRefused<UnauthorizedAccessException>(() => { using (SerialPort none = new SerialPort("/dev/ttyS9", 115200)) { none.Open(); } }), section, "a name past the row's blocks refused with UnauthorizedAccessException");
            // (3) The lines and the frame: DTR latched and read back; a
            // hardware handshake and a seven-bit frame the row cannot run
            // refused, the port whole after.
            port.DtrEnable = true;
            Check(port.DtrEnable, section, "DTR set and read back");
            Check(SerialRefused<ArgumentException>(() => port.Handshake = Handshake.RequestToSend), section, "a hardware handshake refused with ArgumentException");
            Check(SerialRefused<ArgumentException>(() => port.DataBits = 7), section, "seven data bits refused with ArgumentException");
            Check(port.IsOpen && port.BaudRate == 115200, section, "the port whole after the refusals, at " + port.BaudRate);
            // (4) The rates: past the divider's floor refused; a rate outside
            // the standard codes accepted through the numeric form; a standard
            // code set; the claim's rate restored.
            Check(SerialRefused<IOException>(() => port.BaudRate = 2000000), section, "2000000 baud refused with IOException");
            port.BaudRate = 250000;
            Check(port.IsOpen, section, "250000 baud accepted through the numeric form");
            port.BaudRate = 921600;
            Check(port.BaudRate == 921600, section, "921600 baud set: " + port.BaudRate);
            port.BaudRate = 115200;
            Check(port.BaudRate == 115200, section, "115200 baud restored: " + port.BaudRate);
            // (5) A silent line: a read bounded by ReadTimeout ends in the
            // class's timeout exception.
            port.ReadTimeout = 300;
            byte[] one = new byte[1];
            Check(SerialRefused<TimeoutException>(() => port.Read(one, 0, 1)), section, "a read on a silent line timed out with TimeoutException");
            // (6) Eight bytes out and back through the echo: the count exactly
            // eight, the bytes equal, nothing left.
            byte[] eight = { 1, 2, 3, 4, 5, 6, 7, 8 };
            port.Write(eight, 0, eight.Length);
            Check(WaitFor(() => port.BytesToRead >= 8, 2000), section, "eight bytes echoed within two seconds: BytesToRead " + port.BytesToRead);
            Check(port.BytesToRead == 8, section, "BytesToRead exactly 8 after the echo: " + port.BytesToRead);
            byte[] back = new byte[8];
            int got = ReadWhole(port, back);
            bool same = got == 8;
            for (int i = 0; i < 8 && same; i++)
            {
                same = back[i] == eight[i];
            }
            Check(same, section, "the eight bytes read back equal (" + got + " read)");
            Check(port.BytesToRead == 0, section, "nothing left after the read: " + port.BytesToRead);
            // (7) The event: sixty-four bytes announced by DataReceived and
            // read back in order.
            SerialDataReceivedEventHandler handler = (sender, args) =>
            {
                if (args.EventType == SerialData.Chars)
                {
                    Interlocked.Increment(ref s_serialEvents);
                }
            };
            port.DataReceived += handler;
            byte[] many = new byte[64];
            for (int i = 0; i < many.Length; i++)
            {
                many[i] = (byte)i;
            }
            port.Write(many, 0, many.Length);
            Check(WaitFor(() => Volatile.Read(ref s_serialEvents) > 0, 2000), section, "DataReceived raised within two seconds (" + Volatile.Read(ref s_serialEvents) + " events)");
            Check(WaitFor(() => port.BytesToRead >= 64, 2000), section, "sixty-four bytes echoed within two seconds: BytesToRead " + port.BytesToRead);
            byte[] all = new byte[64];
            int gotAll = ReadWhole(port, all);
            bool ordered = gotAll == 64;
            for (int i = 0; i < 64 && ordered; i++)
            {
                ordered = all[i] == (byte)i;
            }
            Check(ordered, section, "the sixty-four bytes read back in order (" + gotAll + " read)");
            port.DataReceived -= handler;
            // (8) The break: this row has no break control, so the request is
            // a no-op (one line from the host).
            port.BreakState = true;
            Check(port.IsOpen, section, "the break request a no-op on a row without break control");
            // (9) Closed, then opened again: the close gives the block back.
            port.Close();
            Check(!port.IsOpen, section, "closed");
            port.Open();
            Check(port.IsOpen, section, "opened again after the close");
        }

        // The stock class's disposal waits for its event loop to finish its work, not for that
        // thread's exit, and the thread's exit prints a collector line: a bounded wait lets the
        // thread end before this section's PASS line, so the two never share the wire.
        Thread.Sleep(200);

        if (s_failures == before)
        {
            Pass(section, "(the standard serial port class over the device files on the doors: two ports; 115200 8N1; the refusals by their exceptions; a silent read TimeoutException; 8 and 64 bytes echoed with DataReceived; the break a no-op; reopened)");
        }
    }

    // The analog section's helpers: the device-file tree the class reads, and a refused open by its
    // exception type and a word of its message.
    private const string IioDevices = "/sys/bus/iio/devices/iio:device";

    private static bool Refused<T>(Action call, string word) where T : Exception
    {
        try
        {
            call();
            return false;
        }
        catch (T e)
        {
            return word.Length == 0 || e.Message.Contains(word);
        }
        catch (Exception)
        {
            return false;
        }
    }

    private static int RawFile(int device, int channel)
    {
        // A read of a raw file at the start of the file is one conversion, claimed and released in the one call.
        return int.Parse(File.ReadAllText(IioDevices + device + "/in_voltage" + channel + "_raw").Trim(), CultureInfo.InvariantCulture);
    }

    private static int RawCount(FileStream raw)
    {
        // One conversion through a descriptor held open: the read at the start of the file, unbuffered, the count as text.
        byte[] text = new byte[32];
        raw.Seek(0, SeekOrigin.Begin);
        int n = raw.Read(text, 0, text.Length);
        return int.Parse(Encoding.ASCII.GetString(text, 0, n).Trim(), CultureInfo.InvariantCulture);
    }

    // The nineteenth section drives the board class's analog controller, the standard analog classes over the IIO
    // device files the host presents on the kernel's ADC doors: the emulator row's converter (12 bits of 3.3 V on
    // the first four lines) through the controller, and its second converter (14 bits of 3.0 V: the reference and
    // the loop channel, which read no pin) through the files themselves. No voltage member computes here: the row's
    // switch is false, so the three throw, and the image carries no code of the library they would compute in.
    private static void Section19Adc()
    {
        const string section = "rt-adc";
        int before = s_failures;
        TargetBoard board = new TargetBoard();
        using (AnalogController adc = board.CreateAnalogController(0))
        {
            // (1) The controller's facts: the package's six lines, the first four with an analog input, the channel
            // numbers both ways.
            Check(adc.PinCount == 6 && adc.SupportsAnalogInput(0) && adc.SupportsAnalogInput(3) && !adc.SupportsAnalogInput(4) && !adc.SupportsAnalogInput(5) && !adc.SupportsAnalogInput(6), section, "the controller numbers " + adc.PinCount + " lines, 0 to 3 with an analog input");
            Check(adc.ConvertPinNumberToAnalogChannelNumber(2) == 2 && adc.ConvertPinNumberToAnalogChannelNumber(5) == -1 && adc.ConvertAnalogChannelNumberToPinNumber(3) == 3 && adc.ConvertAnalogChannelNumberToPinNumber(9) == -1, section, "the channel numbers both ways: line 2 channel 2, line 5 none, channel 3 line 3, channel 9 none");
            // (2) Line 0: one conversion per read (the model's 819) as a count, 12 bits; the scale file's text; the
            // three voltage members and the value-changed event refused with their sentences.
            using (AnalogInputPin pin = adc.OpenPin(0))
            {
                uint first = pin.ReadRaw();
                uint second = pin.ReadRaw();
                Check(first == 819 && second == 819, section, "line 0 reads 819 twice: " + first + ", " + second);
                Check(pin.AdcResolutionBits == 12 && pin.PinNumber == 0 && adc.IsPinOpen(0), section, "the pin's facts: " + pin.AdcResolutionBits + " bits on line " + pin.PinNumber + ", open on the controller");
                string scale = File.ReadAllText(IioDevices + "0/in_voltage_scale");
                Check(scale == "0.805664062\n", section, "the scale file reads " + scale.Trim());
                Check(Refused<NotSupportedException>(() => pin.ReadVoltage(), "ReadRaw") && Refused<NotSupportedException>(() => _ = pin.MinVoltage, "MinVoltage") && Refused<NotSupportedException>(() => _ = pin.MaxVoltage, "MaxVoltage"), section, "the three voltage members refused with NotSupportedException naming the board and ReadRaw");
                Check(Refused<NotSupportedException>(() => pin.EnableAnalogValueChangedEvent(null, 0), "not supported"), section, "the value-changed event refused with NotSupportedException");
                // (3) A second open of the held line: the standard class's own refusal on this controller; the
                // file's EBUSY on another controller, naming the hold.
                Check(Refused<InvalidOperationException>(() => adc.OpenPin(0), "already open"), section, "a second open of line 0 on the same controller refused by the standard class");
                using (AnalogController other = board.CreateAnalogController(0))
                {
                    Check(Refused<IOException>(() => other.OpenPin(0), "held"), section, "another controller's open of the held line refused with IOException naming the hold");
                }
            }

            Check(!adc.IsPinOpen(0), section, "line 0 closed on the controller after the pin's disposal");
            using (AnalogInputPin again = adc.OpenPin(0))
            {
                uint count = again.ReadRaw();
                Check(count == 819, section, "line 0 opens again after the disposal and reads " + count);
            }

            // (4) A line without the function, refused by the standard class before any file is opened; a chip the
            // board does not have, refused by the board class.
            Check(Refused<NotSupportedException>(() => adc.OpenPin(5), "not supporting"), section, "line 5 refused with NotSupportedException");
            Check(Refused<NotSupportedException>(() => board.CreateAnalogController(1), "chip 0"), section, "chip 1 refused with NotSupportedException naming chip 0");
            // (5) Both directions: a line a program holds as a GPIO line refuses the channel until the line is
            // closed; a held channel refuses the line until it is disposed; both give back.
            using (GpioController gpio = new GpioController())
            {
                gpio.OpenPin(1, PinMode.Input);
                Check(Refused<IOException>(() => adc.OpenPin(1), "GPIO line"), section, "line 1 refused with IOException while a program holds it as a GPIO line");
                gpio.ClosePin(1);
                using (AnalogInputPin one = adc.OpenPin(1))
                {
                    uint count = one.ReadRaw();
                    Check(count == 1638, section, "line 1 reads 1638 after the line's close: " + count);
                    string refusal = "none";
                    try
                    {
                        gpio.OpenPin(1, PinMode.Input);
                        gpio.ClosePin(1);
                    }
                    catch (Exception e)
                    {
                        refusal = e.GetType().Name;
                    }

                    Check(refusal == "IOException", section, "the GPIO open of line 1 while the channel is held: " + refusal + " (IOException expected)");
                }

                gpio.OpenPin(1, PinMode.Input);
                Check(gpio.IsPinOpen(1), section, "line 1 opens as a GPIO line after the channel's disposal");
                gpio.ClosePin(1);
            }

            // (6) A pin left open: the controller's disposal closes it (checked after the block).
            AnalogInputPin left = adc.OpenPin(2);
            Check(left.ReadRaw() == 2457 && adc.IsPinOpen(2), section, "line 2 reads 2457 and stays open on the controller");
        }

        using (GpioController gpio = new GpioController())
        {
            gpio.OpenPin(2, PinMode.Input);
            Check(gpio.IsPinOpen(2), section, "line 2 opens as a GPIO line after the controller's disposal closed its pin");
            gpio.ClosePin(2);
        }

        // (7) The second converter, whose channels read no pin, through the files themselves: the reference channel;
        // the loop channel following PWM line 0's duty cycle (the row's virtual loop), 14 bits wide, read through ONE
        // descriptor held open across the duty changes, as the pin class reads (a read at the start of the file is one
        // conversion; a descriptor whose later reads answered its first text would fail the checks that follow), the
        // polarity written inversed while the channel runs.
        int reference = RawFile(1, 0);
        Check(reference == 6619, section, "converter 1's channel 0 reads " + reference + " through its file, 6619 expected");
        using (FileStream loop = new FileStream(IioDevices + "1/in_voltage1_raw", FileMode.Open, FileAccess.Read, FileShare.ReadWrite, 1))
        {
            int idle = RawCount(loop);
            Check(idle == 0, section, "the loop channel reads 0 with no PWM running: " + idle);
            using (PwmChannel pwm = PwmChannel.Create(0, 0, 1000, 0.25))
            {
                pwm.Start();
                int quarter = RawCount(loop);
                Check(quarter == 4096, section, "the loop channel at 25 percent reads " + quarter + ", 4096 expected");
                pwm.DutyCycle = 0.75;
                int three = RawCount(loop);
                Check(three == 12287, section, "the loop channel at 75 percent reads " + three + ", 12287 expected");
                pwm.DutyCycle = 0.25;
                File.WriteAllText(PwmChip + "/pwm0/polarity", "inversed");
                int inversed = RawCount(loop);
                Check(inversed == 12287, section, "the loop channel inversed at 25 percent reads " + inversed + ", 12287 expected");
                pwm.Stop();
                int stopped = RawCount(loop);
                Check(stopped == 0, section, "the loop channel after Stop reads " + stopped);
            }
        }

        // (8) Nothing leaked: line 0 opens once more on a fresh controller after every disposal.
        using (AnalogController last = board.CreateAnalogController(0))
        using (AnalogInputPin pin = last.OpenPin(0))
        {
            uint count = pin.ReadRaw();
            Check(count == 819, section, "line 0 opens once more after every disposal and reads " + count);
        }

        if (s_failures == before)
        {
            Pass(section, "(the standard analog controller over the IIO files: 819 at 12 bits on line 0; the voltage members not supported; a held line refused on both controllers; a held line or channel refuses the other; the disposal gives back; converter 1 by file)");
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
        Guarded("rt-pwm", Section16Pwm);
        Guarded("rt-null", Section17Null);
        Guarded("rt-serial", Section18Serial);
        Guarded("rt-adc", Section19Adc);
        if (s_failures != 0)
        {
            Console.WriteLine("RTSUITE: FAIL (" + s_failures + " checks)");
            return 1;
        }
        Console.WriteLine("RTSUITE: ALL GOOD");
        return 0;
    }
}
