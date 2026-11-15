// The network suite's program: seven sections over the standard socket, name and interface classes, each one
// line the gate reads in order.

using System;
using System.Net;
using System.Net.NetworkInformation;
using System.Net.Sockets;
using System.Threading;
using System.Threading.Tasks;

namespace Rhu.Hosted;

internal static class Program
{
    private const int Bound = 5000;
    private static readonly IPAddress s_peer = new IPAddress(new byte[] { 192, 0, 2, 1 });
    private static readonly IPAddress s_lease = new IPAddress(new byte[] { 192, 0, 2, 2 });
    private static IPAddress s_own = IPAddress.None;
    private static int s_failed;

    private static int Main()
    {
        Console.WriteLine("NETSUITE: start");
        Run("net-lease", Lease);
        Run("net-interfaces", Interfaces);
        Run("net-lookup", Lookup);
        Run("net-stream", Stream);
        Run("net-options", Options);
        Run("net-datagram", Datagram);
        Run("net-async", Awaited);
        Console.WriteLine(s_failed == 0 ? "NETSUITE: ALL GOOD" : "NETSUITE: FAIL (" + s_failed + " of 7 sections)");
        return s_failed == 0 ? 0 : 1;
    }

    private static void Run(string name, Func<string> section)
    {
        try
        {
            Console.WriteLine("PASS " + name + " (" + section() + ")");
        }
        catch (CheckException e)
        {
            s_failed++;
            Console.WriteLine("FAIL " + name + ": " + e.Message);
        }
        catch (Exception e)
        {
            s_failed++;
            string code = e is SocketException s ? " " + s.SocketErrorCode : "";
            string words = e.Message.Replace('\r', ' ').Replace('\n', ' ');
            Console.WriteLine("FAIL " + name + ": " + e.GetType().Name + code + ": " + words);
        }
    }

    private static void Need(bool held, string otherwise)
    {
        if (!held)
        {
            throw new CheckException(otherwise);
        }
    }

    // The two protocol families, then the board's own name asked until it answers the lease's address.
    private static string Lease()
    {
        bool v4 = Socket.OSSupportsIPv4;
        bool v6 = Socket.OSSupportsIPv6;
        Need(v4 && !v6, "the protocol families read IPv4 " + v4 + " and IPv6 " + v6);
        string host = Dns.GetHostName();
        long t0 = Environment.TickCount64;
        while (true)
        {
            IPAddress[] found = Dns.GetHostAddresses(host);
            if (found.Length == 1 && !IPAddress.IsLoopback(found[0]))
            {
                s_own = found[0];
                break;
            }

            Need(Environment.TickCount64 - t0 < 30000, "the board's own name " + host + " answered no lease within 30 s");
            Thread.Sleep(100);
        }

        Need(s_own.Equals(s_lease), "the board's own name answered " + s_own + " where " + s_lease + " was expected");
        return "IPv4 alone; the board's own name " + host + " answered the lease's address " + s_own;
    }

    // The interface list: the loopback and the medium, the medium's type, state, hardware address and address.
    private static string Interfaces()
    {
        NetworkInterface[] list = NetworkInterface.GetAllNetworkInterfaces();
        Need(list.Length == 2, "the interface list holds " + list.Length + " interfaces where 2 were expected");
        NetworkInterface lo = list[0];
        NetworkInterface eth = list[1];
        Need(lo.Name == "lo" && lo.NetworkInterfaceType == NetworkInterfaceType.Loopback, "the first interface reads " + lo.Name + ", " + lo.NetworkInterfaceType);
        Need(
            eth.Name == "eth0" && eth.NetworkInterfaceType == NetworkInterfaceType.Ethernet && eth.OperationalStatus == OperationalStatus.Up,
            "the second interface reads " + eth.Name + ", " + eth.NetworkInterfaceType + ", " + eth.OperationalStatus);
        string hardware = eth.GetPhysicalAddress().ToString();
        UnicastIPAddressInformationCollection addresses = eth.GetIPProperties().UnicastAddresses;
        Need(addresses.Count == 1, "eth0 holds " + addresses.Count + " addresses where 1 was expected");
        UnicastIPAddressInformation first = addresses[0];
        Need(first.Address.Equals(s_own) && first.IPv4Mask.ToString() == "255.255.255.0", "eth0's address reads " + first.Address + " and the mask " + first.IPv4Mask);
        return "lo and eth0; eth0 up, an Ethernet interface at " + hardware + " with " + first.Address + " and the mask " + first.IPv4Mask;
    }

    // Three names the peer answers, one of them through the asynchronous call, and a name it does not have.
    private static string Lookup()
    {
        IPAddress[] one = Dns.GetHostAddresses("peer.rhu.test");
        Need(one.Length == 1, "peer.rhu.test answered " + Count(one.Length) + " where 1 was expected");
        IPAddress[] two = Dns.GetHostAddresses("pair.rhu.test");
        Need(two.Length == 2, "pair.rhu.test answered " + Count(two.Length) + " where 2 were expected");
        IPAddress[] three = Dns.GetHostAddressesAsync("second.rhu.test").GetAwaiter().GetResult();
        Need(three.Length == 1, "second.rhu.test answered " + Count(three.Length) + " where 1 was expected");
        Need(
            one[0].ToString() == "192.0.2.1" && two[0].ToString() == "192.0.2.5" && two[1].ToString() == "192.0.2.6" && three[0].ToString() == "192.0.2.3",
            "the names answered " + one[0] + ", " + two[0] + " and " + two[1] + ", " + three[0]);
        SocketError unknown = SocketError.Success;
        try
        {
            Dns.GetHostAddresses("nowhere.rhu.test");
        }
        catch (SocketException e)
        {
            unknown = e.SocketErrorCode;
        }

        Need(unknown == SocketError.HostNotFound, "a name the server has not ended " + unknown + " where HostNotFound was expected");
        return "peer.rhu.test " + one[0] + "; pair.rhu.test " + two[0] + " and " + two[1] + " in the answer's order; second.rhu.test " + three[0]
            + " through the asynchronous call; a name the server has not " + unknown;
    }

    // A listener's accept of a client's connect over the board's own address, bytes both ways, the shutdown.
    private static string Stream()
    {
        var listener = new TcpListener(IPAddress.Any, 7777);
        listener.Start(1);
        try
        {
            listener.Server.ReceiveTimeout = Bound;
            using var client = new TcpClient(AddressFamily.InterNetwork);
            client.SendTimeout = Bound;
            client.ReceiveTimeout = Bound;
            client.Connect(s_own, 7777);
            using Socket served = listener.AcceptSocket();
            served.ReceiveTimeout = Bound;
            served.SendTimeout = Bound;
            var from = (IPEndPoint)served.RemoteEndPoint!;
            var local = (IPEndPoint)client.Client.LocalEndPoint!;
            Need(from.Address.Equals(s_own) && from.Port == local.Port, "the accepted socket's peer reads " + from + " where the client's own " + local + " was expected");
            NetworkStream stream = client.GetStream();
            byte[] letters = Letters();
            stream.Write(letters, 0, letters.Length);
            byte[] got = new byte[64];
            int n = 0;
            while (n < letters.Length)
            {
                int k = served.Receive(got, n, got.Length - n, SocketFlags.None);
                Need(k > 0, "the stream ended at " + n + " of 26 bytes");
                n += k;
            }

            Need(n == letters.Length && Same(got, letters, letters.Length), "the 26 bytes were not read as sent");
            byte[] pattern = new byte[3000];
            for (int i = 0; i < pattern.Length; i++)
            {
                pattern[i] = (byte)((i * 7) + 3);
            }

            int sent = 0;
            while (sent < pattern.Length)
            {
                sent += served.Send(pattern, sent, pattern.Length - sent, SocketFlags.None);
            }

            byte[] back = new byte[pattern.Length];
            int read = 0;
            while (read < back.Length)
            {
                int k = stream.Read(back, read, back.Length - read);
                Need(k > 0, "the stream ended at " + read + " of 3000 bytes");
                read += k;
            }

            Need(Same(back, pattern, pattern.Length), "the 3000 bytes were not read as sent");
            client.Client.Shutdown(SocketShutdown.Send);
            Need(served.Receive(got, 0, got.Length, SocketFlags.None) == 0, "the client's shutdown was not read as the end of the stream");
        }
        finally
        {
            listener.Stop();
        }

        return "a listener on port 7777 accepted the client from the board's own address; 26 bytes and 3000 bytes read byte-exact; the client's shutdown read as the end of the stream";
    }

    // The pending error, the linger state and the two timeouts, each through the socket class's own member.
    private static string Options()
    {
        var refusing = new IPEndPoint(s_peer, 7);
        SocketError blocking;
        using (var c = new Socket(AddressFamily.InterNetwork, SocketType.Stream, ProtocolType.Tcp))
        {
            c.SendTimeout = Bound;
            blocking = Connect(c, refusing);
        }

        Need(blocking == SocketError.ConnectionRefused, "the blocking connect to the peer's port 7 ended " + blocking + " where ConnectionRefused was expected");
        SocketError pending;
        using (var c = new Socket(AddressFamily.InterNetwork, SocketType.Stream, ProtocolType.Tcp))
        {
            c.Blocking = false;
            SocketError started = Connect(c, refusing);
            Need(started == SocketError.WouldBlock || started == SocketError.InProgress, "the non-blocking connect answered " + started + " at once");
            Need(c.Poll(Bound * 1000, SelectMode.SelectWrite), "the non-blocking connect's end was not reported within 5 s");
            pending = (SocketError)(int)c.GetSocketOption(SocketOptionLevel.Socket, SocketOptionName.Error)!;
        }

        Need(pending == SocketError.ConnectionRefused, "the pending error read " + pending + " where ConnectionRefused was expected");
        using var listener = new Socket(AddressFamily.InterNetwork, SocketType.Stream, ProtocolType.Tcp);
        listener.Bind(new IPEndPoint(IPAddress.Any, 7778));
        listener.Listen(1);
        listener.ReceiveTimeout = Bound;
        using var client = new Socket(AddressFamily.InterNetwork, SocketType.Stream, ProtocolType.Tcp);
        client.SendTimeout = Bound;
        client.Connect(new IPEndPoint(s_own, 7778));
        using Socket served = listener.Accept();
        client.LingerState = new LingerOption(true, 3);
        LingerOption linger = client.LingerState!;
        Need(linger.Enabled && linger.LingerTime == 3, "the linger state read back " + linger.Enabled + ", " + linger.LingerTime + " s");
        client.ReceiveTimeout = 300;
        client.SendTimeout = 400;
        Need(client.ReceiveTimeout == 300 && client.SendTimeout == 400, "the two timeouts read back " + client.ReceiveTimeout + " and " + client.SendTimeout + " ms");
        long t0 = Environment.TickCount64;
        SocketError idle = SocketError.Success;
        try
        {
            client.Receive(new byte[8]);
        }
        catch (SocketException e)
        {
            idle = e.SocketErrorCode;
        }

        long waited = Environment.TickCount64 - t0;
        Need(idle == SocketError.TimedOut && waited >= 250 && waited < Bound, "a receive with nothing to read ended " + idle + " after " + waited + " ms where TimedOut at 300 ms was expected");
        client.LingerState = new LingerOption(false, 0);
        return "the peer's reset read ConnectionRefused by a blocking connect and as the pending error after a non-blocking one; the linger state and the two timeouts set and read back; "
            + "a receive with nothing to read TimedOut at its bound";
    }

    // Two bound datagram sockets: a datagram each way with the sender's address, one too long to send, and an
    // IPv6 socket asked while both are held.
    private static string Datagram()
    {
        using var a = new UdpClient(7001);
        using var b = new UdpClient(7002);
        a.Client.ReceiveTimeout = Bound;
        b.Client.ReceiveTimeout = Bound;
        byte[] message = new byte[64];
        for (int i = 0; i < message.Length; i++)
        {
            message[i] = (byte)((i * 5) + 1);
        }

        a.Send(message, message.Length, new IPEndPoint(s_own, 7002));
        IPEndPoint? from = new IPEndPoint(IPAddress.Any, 0);
        byte[] got = b.Receive(ref from);
        Need(got.Length == message.Length && Same(got, message, message.Length), "the datagram was read as " + got.Length + " bytes, not the 64 sent");
        Need(from.Address.Equals(s_own) && from.Port == 7001, "the datagram's sender reads " + from + " where " + s_own + ":7001 was expected");
        b.Send(got, got.Length, from);
        IPEndPoint? back = new IPEndPoint(IPAddress.Any, 0);
        byte[] echo = a.Receive(ref back);
        Need(echo.Length == message.Length && Same(echo, message, message.Length) && back.Port == 7002, "the answer was read as " + echo.Length + " bytes from " + back);
        SocketError big = SocketError.Success;
        try
        {
            a.Client.SendTo(new byte[2000], new IPEndPoint(s_own, 7002));
        }
        catch (SocketException e)
        {
            big = e.SocketErrorCode;
        }

        Need(big == SocketError.MessageSize, "a 2000-byte datagram ended " + big + " where MessageSize was expected");
        SocketError six = SocketError.Success;
        try
        {
            new Socket(AddressFamily.InterNetworkV6, SocketType.Dgram, ProtocolType.Udp).Dispose();
        }
        catch (SocketException e)
        {
            six = e.SocketErrorCode;
        }

        Need(six == SocketError.AddressFamilyNotSupported, "an IPv6 socket asked while both datagram sockets are held ended " + six + " where AddressFamilyNotSupported was expected");
        return "two bound datagram sockets: 64 bytes each way with the sender's address; a 2000-byte datagram answered MessageSize; an IPv6 socket asked while both are held " + six;
    }

    // An accept, a connect, a send and a receive awaited: the socket engine and its event port.
    private static string Awaited()
    {
        return AwaitedAsync().GetAwaiter().GetResult();
    }

    private static async Task<string> AwaitedAsync()
    {
        using var bound = new CancellationTokenSource(2 * Bound);
        using var listener = new Socket(AddressFamily.InterNetwork, SocketType.Stream, ProtocolType.Tcp);
        listener.Bind(new IPEndPoint(IPAddress.Any, 7779));
        listener.Listen(1);
        using var client = new Socket(AddressFamily.InterNetwork, SocketType.Stream, ProtocolType.Tcp);
        Task<Socket> accepting = listener.AcceptAsync(bound.Token).AsTask();
        await client.ConnectAsync(new IPEndPoint(s_own, 7779), bound.Token);
        using Socket served = await accepting;
        byte[] letters = Letters();
        int sent = await client.SendAsync(new ReadOnlyMemory<byte>(letters), SocketFlags.None, bound.Token);
        Need(sent == letters.Length, "the awaited send took " + sent + " of 26 bytes");
        byte[] got = new byte[64];
        int n = 0;
        while (n < letters.Length)
        {
            int k = await served.ReceiveAsync(new Memory<byte>(got, n, got.Length - n), SocketFlags.None, bound.Token);
            Need(k > 0, "the stream ended at " + n + " of 26 bytes");
            n += k;
        }

        Need(Same(got, letters, letters.Length), "the 26 bytes were not read as sent");
        await served.SendAsync(new ReadOnlyMemory<byte>(got, 0, letters.Length), SocketFlags.None, bound.Token);
        byte[] back = new byte[64];
        int m = 0;
        while (m < letters.Length)
        {
            int k = await client.ReceiveAsync(new Memory<byte>(back, m, back.Length - m), SocketFlags.None, bound.Token);
            Need(k > 0, "the echo ended at " + m + " of 26 bytes");
            m += k;
        }

        Need(Same(back, letters, letters.Length), "the 26 bytes were not echoed as sent");
        return "an accept, a connect, a send and a receive awaited: 26 bytes echoed";
    }

    private static SocketError Connect(Socket socket, IPEndPoint to)
    {
        try
        {
            socket.Connect(to);
            return SocketError.Success;
        }
        catch (SocketException e)
        {
            return e.SocketErrorCode;
        }
    }

    private static byte[] Letters()
    {
        byte[] letters = new byte[26];
        for (int i = 0; i < letters.Length; i++)
        {
            letters[i] = (byte)('a' + i);
        }

        return letters;
    }

    private static bool Same(byte[] a, byte[] b, int count)
    {
        for (int i = 0; i < count; i++)
        {
            if (a[i] != b[i])
            {
                return false;
            }
        }

        return true;
    }

    private static string Count(int n)
    {
        return n == 1 ? "1 address" : n + " addresses";
    }

    private sealed class CheckException : Exception
    {
        public CheckException(string message)
            : base(message)
        {
        }
    }
}
