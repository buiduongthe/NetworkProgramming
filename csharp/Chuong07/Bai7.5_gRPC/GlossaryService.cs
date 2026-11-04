using System.Net;
using Glossary.V1;
using Grpc.Core;
using Microsoft.AspNetCore.Server.Kestrel.Core;

namespace Bai7_5;

// Mã giả 7.6 – CLASS GlossaryService EXTENDS GlossaryBase (lớp cơ sở do Grpc.Tools sinh từ tra_cuu.proto)
// Kế thừa Mã giả 3.7: nhánh "lookup" của PROCESS_REQUEST thành phương thức Lookup
public sealed class GlossaryService(DelayOption delay) : Glossary.V1.Glossary.GlossaryBase
{
    static readonly Dictionary<string, string> Dict = new()
    {
        ["TCP"] = "Giao thức vận chuyển hướng kết nối",
        ["UDP"] = "Giao thức vận chuyển không kết nối",
    };
    static readonly string[] Changes = ["HTTP/2", "QUIC", "HTTP/3", "gRPC", "HTTPS"];   // dòng thay đổi mô phỏng

    public override async Task<Term> Lookup(LookupRequest req, ServerCallContext ctx)
    {
        if (delay.Ms > 0)
        {
            try { await Task.Delay(delay.Ms, ctx.CancellationToken); }  // server cố ý chậm
            catch (OperationCanceledException)                          // hạn chót truyền qua grpc-timeout
            {
                Console.WriteLine($"[server] Lookup('{req.Term}') bị hủy: client đã hết hạn chót");
                throw;
            }
        }
        if (!Dict.TryGetValue(req.Term, out var text))
        {
            Console.WriteLine($"[server] Lookup('{req.Term}') -> NOT_FOUND");
            throw new RpcException(new Status(StatusCode.NotFound, "Không có " + req.Term));   // RAISE RpcError
        }
        Console.WriteLine($"[server] Lookup('{req.Term}') -> OK");
        return new Term { Term_ = req.Term, Text = text };              // trường 'term' sinh thành Term_
    }

    public override async Task Watch(WatchRequest req, IServerStreamWriter<Term> stream, ServerCallContext ctx)
    {
        Console.WriteLine($"[server] Watch(prefix='{req.Prefix}') bắt đầu");
        await foreach (var t in ChangesFeed(ctx.CancellationToken))
        {
            if (ctx.CancellationToken.IsCancellationRequested) return;  // client đã hủy
            if (t.Term_.StartsWith(req.Prefix))
            {
                await stream.WriteAsync(t);
                Console.WriteLine($"[server] Watch: gửi {t.Term_}");
            }
        }
        Console.WriteLine("[server] Watch kết thúc");
    }

    // CHANGES_FEED(): mỗi 0,5 giây có một thuật ngữ mới
    static async IAsyncEnumerable<Term> ChangesFeed([System.Runtime.CompilerServices.EnumeratorCancellation] CancellationToken ct)
    {
        foreach (var term in Changes)
        {
            try { await Task.Delay(500, ct); } catch (OperationCanceledException) { yield break; }
            yield return new Term { Term_ = term, Text = "Thuật ngữ mới: " + term, Tags = { "moi" } };
        }
    }
}

public sealed record DelayOption(int Ms);

// Máy chủ ASP.NET Core (Kestrel) chỉ bật HTTP/2 trên cổng không TLS (h2c) cho phòng thí nghiệm
static class GlossaryHost
{
    public static async Task<int> RunAsync(string[] args)
    {
        string host = args.Length >= 1 ? args[0] : "0.0.0.0";
        int port = args.Length >= 2 ? int.Parse(args[1]) : 5001;
        int delayMs = args.Length >= 3 ? int.Parse(args[2]) : 0;
        var builder = WebApplication.CreateBuilder();
        builder.Logging.ClearProviders();                               // chỉ in thông điệp của bài
        builder.WebHost.ConfigureKestrel(o =>
            o.Listen(IPAddress.Parse(host), port, l => l.Protocols = HttpProtocols.Http2));
        builder.Services.AddGrpc();
        builder.Services.AddSingleton(new DelayOption(delayMs));
        var app = builder.Build();
        app.MapGrpcService<GlossaryService>();
        Console.WriteLine($"[server] Dịch vụ gRPC glossary.v1.Glossary tại {host}:{port} (độ trễ Lookup {delayMs} ms)");
        await app.RunAsync();
        Console.WriteLine("[server] Dừng server");
        return 0;
    }
}
