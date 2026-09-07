// The smallest hosted program: its markers in order, the verdict line last.
using System;

namespace Rhu.Hosted;

internal static class Program
{
    private sealed class Probe
    {
        public int Value;
    }

    private static int Main(string[] args)
    {
        Console.WriteLine("RTHELLO: start (args " + args.Length + ")");

        int caught = 0;
        try
        {
            throw new InvalidOperationException("thrown on purpose");
        }
        catch (InvalidOperationException)
        {
            caught = 1;
        }
        Console.WriteLine("RTHELLO: exception caught " + caught);

        long sum = 0;
        for (int i = 0; i < 1000; i++)
        {
            Probe p = new Probe { Value = i };
            sum += p.Value;
        }
        GC.Collect();
        Console.WriteLine("RTHELLO: allocated 1000 objects, sum " + sum);
        Console.WriteLine("RTHELLO: collections " + GC.CollectionCount(0));

        Console.WriteLine("RTHELLO: value " + 4660.ToString("X") + " " + 7.5.ToString("F1"));

        if (caught != 1 || sum != 499500)
        {
            Console.WriteLine("RTHELLO: FAIL");
            return 1;
        }
        Console.WriteLine("RTHELLO: ALL GOOD");
        return 0;
    }
}
