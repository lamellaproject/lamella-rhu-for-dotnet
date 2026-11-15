int a = First(3);
int b = Second(4);
int c = DbgOwnLib.Twice.Of(a + b);
Console.WriteLine(c);
static int First(int x)
{
    int y = x + 1;
    return y;
}
static int Second(int x)
{
    int y = x + 1;
    return y;
}
