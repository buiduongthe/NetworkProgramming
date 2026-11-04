using System.Net.Sockets;
using System.Text.Json.Nodes;

namespace Bai5_4;

// Mã giả 5.5 – Lớp dịch vụ ChatClient bất đồng bộ (Mục 5.3.3).
// Kế thừa: Mã giả 5.1 (khung JSON bất đồng bộ); SAFE_CONNECT_ASYNC = Mã giả 2.2 + AWAIT.
// Lớp không tham chiếu tới điều khiển giao diện nào; tầng giao diện chỉ đăng ký OnMessage, OnStatus.
class ChatClient
{
    NetworkStream? sock;                                                // sock
    CancellationTokenSource? cts;                                       // cts: dùng chung cho mọi thao tác
    readonly SemaphoreSlim sendLock = new(1, 1);                        // send_lock ← ASYNC_LOCK()
    public event Action<JsonNode>? OnMessage;                           // sự kiện cho UI
    public event Action<string>? OnStatus;

    public async Task ConnectAsync(string host, int port, string name) // dòng 3–14
    {
        Socket s = await Net.SafeConnectAsync(host, port, 3000);
        sock = new NetworkStream(s, ownsSocket: true);
        cts = new CancellationTokenSource();                            // NEW_CANCELLATION()
        var hello = new JsonObject { ["type"] = "hello", ["name"] = name };
        try
        {
            await Khung.SendJsonAsync(sock, hello, cts.Token);
        }
        catch (IOException)
        {
            sock.Close(); throw;                                        // không để rò socket
        }
        _ = ReceiveLoopAsync();                                         // RUN_BACKGROUND: không await
        OnStatus?.Invoke("connected");
    }

    async Task ReceiveLoopAsync()                                       // dòng 15–27
    {
        try
        {
            while (true)
            {
                JsonNode msg = await Khung.RecvJsonAsync(sock!, cts!.Token);
                OnMessage?.Invoke(msg);                                 // RAISE_EVENT(on_message, msg)
            }
        }
        catch (Exception) when (cts!.IsCancellationRequested)
        {
            return;                                                     // Cancelled: ngắt kết nối chủ động
        }
        catch (Exception e) when (e is NetError || e is ConnectionClosedException || e is ProtocolException
                                  || e is IOException || e is System.Text.Json.JsonException)   // dòng 23: … JsonError
        {
            sock!.Close();                                              // giải phóng socket
            OnStatus?.Invoke("lost: " + e.Message);
        }
    }

    public async Task SendMessageAsync(string room, string text)        // dòng 28–36
    {
        await sendLock.WaitAsync();                                     // mỗi lúc gửi một khung
        try
        {
            var m = new JsonObject { ["type"] = "msg", ["room"] = room, ["text"] = text };
            await Khung.SendJsonAsync(sock!, m, cts!.Token);
        }
        finally
        {
            sendLock.Release();
        }
    }

    public void Disconnect()                                            // dòng 37–39
    {
        cts?.Cancel();                                                  // hủy trước
        sock?.Close();                                                  // đóng socket sau
    }
}
