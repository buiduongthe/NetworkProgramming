using System.Net;
using System.Net.Security;
using System.Net.Sockets;
using System.Security.Authentication;
using System.Security.Cryptography.X509Certificates;
using System.Text.Json.Nodes;

namespace Bai8_4;

// Mã giả 8.6 phía client: CONNECT = SAFE_CONNECT_ASYNC rồi TLS_WRAP_ASYNC(raw, CLIENT, server_name ← host);
// kế thừa Mã giả 5.5 (ChatClient: RECEIVE_LOOP chạy nền, SEND_MESSAGE có khóa gửi)
class ChatClient
{
    SslStream tls = null!;
    readonly SemaphoreSlim sendLock = new(1, 1);                  // mỗi lúc gửi một khung
    readonly TaskCompletionSource<bool> welcome = new();         // true: được chấp nhận; false: lỗi
    readonly TaskCompletionSource pong = new();

    static void Log(string text) => Console.WriteLine("[client] " + text);

    // Mã giả 2.2 – SAFE_CONNECT (rút gọn, bất đồng bộ): thử lần lượt các địa chỉ, chờ tối đa timeoutMs mỗi địa chỉ
    static async Task<Socket> SafeConnectAsync(string host, int port, int timeoutMs)
    {
        Exception? last = null;
        foreach (IPAddress a in await Dns.GetHostAddressesAsync(host))
        {
            var s = new Socket(a.AddressFamily, SocketType.Stream, ProtocolType.Tcp);
            try { await s.ConnectAsync(a, port, new CancellationTokenSource(timeoutMs).Token); return s; }
            catch (Exception e) when (e is SocketException or OperationCanceledException) { last = e; s.Dispose(); }
        }
        throw new IOException($"Không kết nối được {host}:{port}: {last?.Message}");
    }

    async Task Connect(string host, int port, string token, string? caFile)
    {
        Socket raw = await SafeConnectAsync(host, port, 3000);
        tls = new SslStream(new NetworkStream(raw, ownsSocket: true), leaveInnerStreamOpen: false);
        var opts = new SslClientAuthenticationOptions
        { TargetHost = host, EnabledSslProtocols = SslProtocols.Tls12 | SslProtocols.Tls13 };   // server_name ← host
        if (caFile != null)                                       // tin cậy CA nội bộ, vẫn kiểm tra chuỗi và tên máy
        {
            opts.CertificateChainPolicy = new X509ChainPolicy
            { TrustMode = X509ChainTrustMode.CustomRootTrust, RevocationMode = X509RevocationMode.NoCheck };
            opts.CertificateChainPolicy.CustomTrustStore.Add(X509CertificateLoader.LoadCertificateFromFile(caFile));
        }
        await tls.AuthenticateAsClientAsync(opts);               // TLS_WRAP_ASYNC(raw, CLIENT, server_name ← host)
        string ver = tls.SslProtocol == SslProtocols.Tls13 ? "TLSv1.3" : tls.SslProtocol == SslProtocols.Tls12 ? "TLSv1.2" : tls.SslProtocol.ToString();
        Log($"Đã kết nối TLS tới {host}:{port}: {ver}, {tls.NegotiatedCipherSuite}");
        Log($"Chứng chỉ server {tls.RemoteCertificate!.Subject} hợp lệ, tên '{host}' khớp SAN");
        await Send(new JsonObject { ["type"] = "hello", ["token"] = token });
        _ = ReceiveLoop();                                        // RUN_BACKGROUND: không await
    }

    async Task ReceiveLoop()
    {
        try
        {
            while (true)
            {
                JsonObject m = await Frame.RecvJsonAsync(tls);
                switch (Frame.Str(m["type"]))
                {
                    case "welcome":
                        var users = m["users"]!.AsArray().Select(x => Frame.Str(x));
                        Log($"Server chấp nhận: tên = {Frame.Str(m["name"])} (từ token), trực tuyến: {string.Join(", ", users)}");
                        welcome.TrySetResult(true); break;
                    case "error":
                        Log($"Lỗi từ server: {m["code"]} {Frame.Str(m["text"])}");
                        welcome.TrySetResult(false); break;
                    case "sys": Log($"* {Frame.Str(m["text"])}"); break;
                    case "msg": Log($"[{Frame.Str(m["room"])}] {Frame.Str(m["from"])}: {Frame.Str(m["text"])}"); break;
                    case "pong": pong.TrySetResult(); break;
                }
            }
        }
        catch (Exception e) when (e is IOException or EndOfStreamException or ObjectDisposedException or ProtocolError)
        {
            if (!pong.Task.IsCompleted) Log("Server đã đóng kết nối");
            welcome.TrySetResult(false);
            pong.TrySetResult();
        }
    }

    async Task Send(JsonObject obj)                               // SEND_MESSAGE: khóa gửi
    {
        await sendLock.WaitAsync();
        try { await Frame.SendJsonAsync(tls, obj); }
        finally { sendLock.Release(); }
    }

    public static async Task<int> Run(string[] args)
    {
        if (args.Length < 3) { Console.WriteLine("Cách dùng: dotnet run -- client <host> <port> <tệp_token> [tệp_CA]"); return 1; }
        string host = args[0], tokenFile = args[2];
        int port = int.Parse(args[1]);
        string? caFile = args.Length > 3 ? args[3] : null;
        string token = File.ReadAllText(tokenFile).Trim();
        Log($"Tin cậy: {(caFile != null ? "CA trong " + caFile : "kho chứng chỉ của hệ điều hành")}; token: {tokenFile}");
        var c = new ChatClient();
        try { await c.Connect(host, port, token, caFile); }
        catch (AuthenticationException e) { Log($"Bắt tay TLS thất bại: {e.Message}"); return 1; }
        catch (IOException e) { Log($"Không kết nối được: {e.Message}"); return 1; }
        if (!await c.welcome.Task.WaitAsync(TimeSpan.FromSeconds(5))) { c.tls.Dispose(); return 1; }
        await c.Send(new JsonObject { ["type"] = "join", ["room"] = "chung" });
        string? line;
        while ((line = Console.ReadLine()) != null)               // READ_CONSOLE: các dòng cần gửi
            await c.Send(new JsonObject { ["type"] = "msg", ["room"] = "chung", ["text"] = line });
        await c.Send(new JsonObject { ["type"] = "ping" });       // pong về sau mọi tin trước đó
        await c.pong.Task.WaitAsync(TimeSpan.FromSeconds(5));
        c.tls.Dispose();                                          // DISCONNECT
        Log("Đã ngắt kết nối");
        return 0;
    }
}
