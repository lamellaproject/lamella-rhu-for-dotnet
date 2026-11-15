namespace DbgOwnLib;

public static class Twice
{
    public static int Of(int value)
    {
        int twice = Add(value, value);
        return twice;
    }
    static int Add(int x, int y)
    {
        return x + y;
    }
}
