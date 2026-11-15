namespace DbgViaLib;

public static class Via
{
    public static int Of(int value)
    {
        int via = DbgOwnLib.Twice.Of(value);
        return via;
    }
}
