// The runtime's stack helper, read where a program meets it. The class
// library asks the helper before work that may nest deeply: a record's
// generated ToString() asks it, and so does a task before it runs a
// continuation on the thread that completes it. A program may ask it at
// each frame of a descent of its own. On a board's small stacks the helper
// must say yes while there is room and say no while enough is left to
// raise its exception: this program prints what each of the four does, and
// returns 0 when all four are as on a stack with room.
using System;
using System.Runtime.CompilerServices;
using System.Threading;
using System.Threading.Tasks;

namespace Rhu.Hosted;

internal record Reading(string Name, int Number);

internal static class Program
{
    private const string Text = "Reading { Name = abc, Number = 7 }";
    private static int s_hold;
    private static int s_last;
    private static int s_sink;

    private static int Main()
    {
        Console.WriteLine("STACKHELPER: start");
        Record("the thread that runs Main");
        Thread spawned = new Thread(static () => Record("a spawned thread"));
        spawned.Start();
        spawned.Join();
        Continuation();
        Thread deep = new Thread(Descent);
        deep.Start();
        deep.Join();
        int hold = Volatile.Read(ref s_hold);
        Console.WriteLine("STACKHELPER: end; " + hold + " of 4 checks hold");
        return hold == 4 ? 0 : 1;
    }

    // A record's ToString() is generated code that asks the helper first.
    private static void Record(string who)
    {
        string said;
        try
        {
            string text = new Reading("abc", 7).ToString();
            said = "returned \"" + text + "\"";
            if (text == Text)
                Interlocked.Increment(ref s_hold);
        }
        catch (Exception e)
        {
            said = "threw " + e.GetType().Name;
        }

        Console.WriteLine("STACKHELPER: on " + who + " a record's ToString() " + said);
    }

    // A continuation that asks to run synchronously runs on the thread that
    // completes its task when the helper says there is room, and is queued
    // to the pool when it says there is none.
    private static void Continuation()
    {
        TaskCompletionSource<int> source = new TaskCompletionSource<int>();
        int ranOn = 0;
        Task after = source.Task.ContinueWith(_ => { Volatile.Write(ref ranOn, Environment.CurrentManagedThreadId); }, CancellationToken.None, TaskContinuationOptions.ExecuteSynchronously, TaskScheduler.Default);
        int completer = Environment.CurrentManagedThreadId;
        source.SetResult(7);
        after.Wait();
        bool inline = Volatile.Read(ref ranOn) == completer;
        if (inline)
            Interlocked.Increment(ref s_hold);
        Console.WriteLine("STACKHELPER: a continuation asked to run synchronously ran on " + (inline ? "the thread that completed its task" : "another thread"));
    }

    // A descent that asks the helper at each frame, on a thread of its own
    // at the default stack: it goes down while the helper says yes, and the
    // helper's exception is caught where the descent began.
    private static void Descent()
    {
        Volatile.Write(ref s_last, -1);
        string ended;
        try
        {
            int depth = Descend(0);
            ended = "returned from frame " + depth;
        }
        catch (InsufficientExecutionStackException e)
        {
            ended = "ended in " + e.GetType().Name + " and was caught";
        }

        int frames = Volatile.Read(ref s_last) + 1;
        if (frames > 0 && ended.StartsWith("ended in ", StringComparison.Ordinal))
            Interlocked.Increment(ref s_hold);
        Console.WriteLine("STACKHELPER: a descent that asks the stack helper at each frame went " + frames + " frames down, " + ended);
    }

    // A frame of a size a real method has (192 bytes of locals).
    [MethodImpl(MethodImplOptions.NoInlining)]
    private static int Descend(int n)
    {
        Span<byte> pad = stackalloc byte[192];
        pad[0] = (byte)n;
        RuntimeHelpers.EnsureSufficientExecutionStack();
        Volatile.Write(ref s_last, n);
        int below = Descend(n + 1);
        s_sink += pad[0];
        return below + 1;
    }
}
