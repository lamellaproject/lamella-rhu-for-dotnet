// The thread pool's bound, read where a program reads it: the most workers
// the pool will run, one more asked of it and refused, and more work items
// queued than the bound allows at once -- every item runs, and never more
// of them at the same moment than the bound. The build states the bound to
// the runtime from the board's stack pool; this program prints what the
// runtime then says and does.
using System;
using System.Threading;

namespace Rhu.Hosted;

internal static class Program
{
    private static int s_running;
    private static int s_most;
    private static int s_done;

    private static void Item(object? state)
    {
        int now = Interlocked.Increment(ref s_running);
        int seen = Volatile.Read(ref s_most);
        while (now > seen)
        {
            int was = Interlocked.CompareExchange(ref s_most, now, seen);
            if (was == seen)
                break;
            seen = was;
        }

        Thread.Sleep(20);
        Interlocked.Decrement(ref s_running);
        Interlocked.Increment(ref s_done);
        ((CountdownEvent)state!).Signal();
    }

    private static int Main()
    {
        ThreadPool.GetMaxThreads(out int workers, out int completionPorts);
        Console.WriteLine("POOLBOUND: the pool runs at most " + workers + (workers == 1 ? " worker" : " workers"));
        bool raised = ThreadPool.SetMaxThreads(workers + 1, completionPorts);
        ThreadPool.GetMaxThreads(out int after, out _);
        Console.WriteLine("POOLBOUND: one more asked: " + (raised ? "taken" : "refused") + "; the most is " + after);
        int items = 2 * workers + 2;
        using var all = new CountdownEvent(items);
        for (int i = 0; i < items; i++)
            ThreadPool.QueueUserWorkItem(Item, all);
        bool ended = all.Wait(60000);
        int most = Volatile.Read(ref s_most);
        bool within = most >= 1 && most <= workers;
        Console.WriteLine("POOLBOUND: " + Volatile.Read(ref s_done) + " of " + items + " work items ran" + (ended ? "" : " (the wait timed out)") + "; never more at once than the bound: " + (within ? "true" : "false, " + most));
        Console.WriteLine("POOLBOUND: end");
        return ended && within && !raised && after == workers ? 0 : 1;
    }
}
