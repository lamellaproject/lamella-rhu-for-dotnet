// The hosted-runtime suite: numbered sections printing PASS, the verdict line last.
using System;
using System.Collections.Generic;
using System.Globalization;
using System.Runtime.CompilerServices;
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
        if (s_failures != 0)
        {
            Console.WriteLine("RTSUITE: FAIL (" + s_failures + " checks)");
            return 1;
        }
        Console.WriteLine("RTSUITE: ALL GOOD");
        return 0;
    }
}
