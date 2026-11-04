using Glossary.V1;
using Google.Protobuf;
using Grpc.Core;
using Grpc.Net.Client;

namespace Bai7_5;

// Mã giả 7.6 – phần "Phía client": Lookup có hạn chót, Watch nhận luồng; GlossaryClient do Grpc.Tools sinh
static class GlossaryCaller
{
    public static async Task<int> RunAsync(string[] args)
    {
        string host = args.Length >= 1 ? args[0] : "127.0.0.1";
        int port = args.Length >= 2 ? int.Parse(args[1]) : 5001;
        int deadlineMs = args.Length >= 3 ? int.Parse(args[2]) : 2000;
        byte[] wire = new LookupRequest { Term = "TCP" }.ToByteArray();     // mã hóa Protocol Buffers (Mục 7.4.2)
        Console.WriteLine($"[client] LookupRequest{{term: \"TCP\"}} = {Convert.ToHexString(wire)} ({wire.Length} byte)");
        using var ch = GrpcChannel.ForAddress($"http://{host}:{port}");   // GRPC_CHANNEL (h2c, không TLS)
        var c = new Glossary.V1.Glossary.GlossaryClient(ch);
        Console.WriteLine($"[client] Gọi Lookup với hạn chót {deadlineMs} ms");
        await Lookup(c, "TCP", deadlineMs);
        await Lookup(c, "XYZ", deadlineMs);
        Console.WriteLine("[client] Gọi Watch(prefix='HTTP'), nhận luồng:");
        try
        {
            using var call = c.Watch(new WatchRequest { Prefix = "HTTP" });
            await foreach (var x in call.ResponseStream.ReadAllAsync())  // FOR EACH x IN AWAIT_STREAM(...)
                Console.WriteLine($"[client]   {x.Term_} – {x.Text}");
        }
        catch (RpcException e) { Console.WriteLine($"[client] Watch -> lỗi {e.StatusCode}: {e.Status.Detail}"); }
        Console.WriteLine("[client] Kết thúc");
        return 0;
    }

    static async Task Lookup(Glossary.V1.Glossary.GlossaryClient c, string term, int deadlineMs)
    {
        try
        {
            var t = await c.LookupAsync(new LookupRequest { Term = term },
                                        deadline: DateTime.UtcNow.AddMilliseconds(deadlineMs));   // deadline ← 2000
            Console.WriteLine($"[client] Lookup('{term}') -> {t.Term_}: {t.Text}");
        }
        catch (RpcException e)                                          // NOT_FOUND, DEADLINE_EXCEEDED, UNAVAILABLE…
        {
            Console.WriteLine($"[client] Lookup('{term}') -> lỗi {e.StatusCode}: {e.Status.Detail}");
        }
    }
}
