using System.Runtime.CompilerServices;

namespace DbgVia;

public static class Kept
{
    [MethodImpl(MethodImplOptions.AggressiveOptimization)]
    public static int Of(int value)
    {
        int kept = Add(value, value);
        return kept;
    }
    [MethodImpl(MethodImplOptions.AggressiveOptimization)]
    static int Add(int x, int y)
    {
        return x + y;
    }
}
