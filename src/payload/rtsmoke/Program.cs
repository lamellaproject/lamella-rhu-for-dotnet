// rtsmoke: the payload gate's boot smoke program against the minimal runtime.

using System.Runtime;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;

class Animal
{
    public virtual int Legs() => 0;
    public virtual int Sound() => '?';
}

class Dog : Animal
{
    public override int Legs() => 4;
    public override int Sound() => 'W';
}

class Bird : Animal
{
    public override int Legs() => 2;
    public override int Sound() => 'T';
}

class Box<T>
{
    public T Value;
    public Box(T v) { Value = v; }
}

unsafe class Program
{
    [DllImport("shim")]
    static extern void sh_write0(byte* asciiz);

    static int s_pass;
    static int s_fail;

    static int Main()
    {
        Print("=== rtsmoke: minirt v2 (alloc/div/casts) ===\n");

        Check("div32", 1234567 / 89 == 13871 && 1234567 % 89 == 48);
        Check("div32neg", -1000 / 3 == -333 && -1000 % 3 == -1);
        Check("divu32", 0xFFFF_FFFFu / 16 == 0x0FFF_FFFFu);
        Check("div64", 3_000_000_021L / 7 == 428_571_431L && 3_000_000_021L % 7 == 4);
        Check("div64neg", -3_000_000_021L / 7 == -428_571_431L && -3_000_000_021L % 7 == -4);
        Check("divu64", 0xFFFF_FFFF_FFFFul / 0x1000 == 0xF_FFFF_FFFFul);

        int sh = Opaque(20);
        Check("lrsz", (0xFFFF_FFFF_FFFFul >> sh) == 0xFFF_FFFFul);
        Check("llsh", (1ul << Opaque(40)) == 0x100_0000_0000ul);
        Check("lrsh", (-4_398_046_511_104L >> Opaque(10)) == -4_294_967_296L);

        Animal a1 = new Dog();
        Animal a2 = new Bird();
        Check("alloc-virt", a1.Legs() == 4 && a2.Legs() == 2 && a1.Sound() == 'W');

        int[] xs = new int[16];
        for (int i = 0; i < xs.Length; i++)
            xs[i] = (i + 1) * (i + 1);
        int sum = 0;
        foreach (int x in xs)
            sum += x;
        Check("int[]", sum == 1496 && xs.Length == 16);

        long[] ls = new long[5];
        for (int i = 0; i < 5; i++)
            ls[i] = 1_000_000_007L * (i + 1);
        Check("long[]", ls[4] / 5 == 1_000_000_007L);

        Animal[] zoo = new Animal[3];
        zoo[0] = a1;
        zoo[1] = a2;
        zoo[2] = new Dog();
        int legs = 0;
        for (int i = 0; i < zoo.Length; i++)
            legs += zoo[i].Legs();
        Check("obj[]", legs == 10);

        Animal back = zoo[1];
        Check("is-cast", back is Bird && !(back is Dog));
        Dog? maybe = zoo[2] as Dog;
        Check("as-cast", maybe != null && maybe.Legs() == 4);

        Box<int> bi = new Box<int>(12345);
        Box<long> bl = new Box<long>(0x1_0000_0001L);
        bi.Value += 55;
        Check("generics", bi.Value == 12400 && bl.Value / 2 == 0x8000_0000L);

        string s = "minirt-v2";
        Check("string", s.Length == 9);

        Print("rtsmoke: ");
        PrintDec(s_pass);
        Print(" pass, ");
        PrintDec(s_fail);
        Print(s_fail == 0 ? " fail - ALL GOOD\n" : " fail - BROKEN\n");
        return s_fail;
    }

    static void Check(string name, bool ok)
    {
        Print(ok ? "PASS " : "FAIL ");
        Print(name);
        Print("\n");
        if (ok) s_pass++; else s_fail++;
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    static int Opaque(int v) => v;

    static void PrintDec(int v)
    {
        byte* b = stackalloc byte[12];
        int n = 0;
        if (v == 0) b[n++] = (byte)'0';
        byte* tmp = stackalloc byte[12];
        int t = 0;
        while (v > 0)
        {
            tmp[t++] = (byte)('0' + v % 10);
            v /= 10;
        }
        while (t > 0) b[n++] = tmp[--t];
        b[n] = 0;
        sh_write0(b);
    }

    static void Print(string s)
    {
        byte* buf = stackalloc byte[96];
        fixed (char* p = s)
        {
            int n = s.Length < 95 ? s.Length : 95;
            for (int i = 0; i < n; i++)
                buf[i] = (byte)p[i];
            buf[n] = 0;
        }
        sh_write0(buf);
    }
}
