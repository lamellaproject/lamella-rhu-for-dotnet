// One exception nothing catches, thrown on the main thread two calls below
// Main. A handler of the program's own stands on the unhandled-exception
// event and prints a line, so the console says what had run when a debugger
// stopped at the event; the finally around the call prints a line too, and
// the runtime ends the program without running it.
using System;
using System.Runtime.CompilerServices;

namespace Rhu.Hosted;

internal static class Program
{
    [MethodImpl(MethodImplOptions.NoInlining)]
    private static void ThrowIt(string message)
    {
        throw new InvalidOperationException(message);
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    private static int Outer(int depth)
    {
        if (depth > 0)
            return Outer(depth - 1) + 1;
        ThrowIt("the one, unhandled");
        return 0;
    }

    private static int Main()
    {
        AppDomain.CurrentDomain.UnhandledException += (sender, e) =>
            Console.WriteLine("UNHANDLED: 3 the program's own handler ran for " + e.ExceptionObject.GetType().FullName);
        Console.WriteLine("UNHANDLED: 1 started");
        try
        {
            Console.WriteLine("UNHANDLED: 2 throwing what nothing catches");
            Outer(1);
        }
        finally
        {
            Console.WriteLine("UNHANDLED: the finally ran");
        }
        Console.WriteLine("UNHANDLED: not reached");
        return 0;
    }
}
