// Bài 8.1 – Gọi API LLM có truyền luồng SSE, giới hạn tần suất, thử lại và hạn chót
// Mã giả 8.1 (llm-stream), 8.2 (llm-call); Mục 8.2 Giáo trình Lập trình mạng
// Cấu hình: LLM_BASE_URL (mặc định http://127.0.0.1:9000/v1 – server giả lập), LLM_API_KEY (bắt buộc),
//           LLM_MODEL (mặc định mock-model), LLM_PRICE_IN, LLM_PRICE_OUT (USD/1 triệu token, tùy chọn)
// Chạy:  python3 mock_llm_server.py 127.0.0.1 9000 --fail-first 1           (cửa sổ 1)
//        dotnet run -- "So sánh TCP và UDP."                                 (cửa sổ 2, đã đặt LLM_API_KEY)
using System.Diagnostics;
using System.Globalization;
using System.Text.Json.Nodes;
using Bai8_1;

CultureInfo.DefaultThreadCurrentCulture = CultureInfo.InvariantCulture;   // số thập phân dùng dấu chấm
Console.OutputEncoding = System.Text.Encoding.UTF8;
string model = Environment.GetEnvironmentVariable("LLM_MODEL") ?? "mock-model";
string question = args.Length > 0 ? string.Join(" ", args) : "So sánh TCP và UDP.";
var req = new JsonObject
{
    ["model"] = model, ["stream"] = true, ["max_tokens"] = 512,
    ["messages"] = new JsonArray(
        new JsonObject { ["role"] = "system", ["content"] = "Trợ lý mạng, trả lời ngắn." },
        new JsonObject { ["role"] = "user", ["content"] = question })
};
Console.WriteLine($"[llm] Mô hình {model} tại {LlmCall.BaseUrl}");
Console.WriteLine($"[llm] Câu hỏi: {question}");
try { LlmCall.Secret("LLM_API_KEY"); }
catch (InvalidOperationException e) { Console.WriteLine($"[llm] {e.Message}"); return 2; }

var clock = Stopwatch.StartNew();
double? first = null;
void OnToken(string d)                                   // UI: UI_INVOKE(APPEND, d)
{
    if (first == null) { first = clock.Elapsed.TotalMilliseconds; Console.Write("[llm] Trả lời: "); }
    Console.Write(d);
}

LlmStream.State st;
try
{
    st = await LlmCall.CallLlm(req, OnToken, CancellationToken.None);     // ct: chương trình dòng lệnh không hủy
}
catch (NetError e)
{
    if (first != null) Console.WriteLine();
    string note = first != null ? " – không thử lại vì đã hiển thị một phần câu trả lời" : "";
    Console.WriteLine($"[llm] Lỗi: {e.Message}{note}");
    return 1;
}
double total = clock.Elapsed.TotalMilliseconds;
Console.WriteLine();
Console.WriteLine($"[llm] TTFT: {first:F0} ms, tổng thời gian: {total:F0} ms");
int nIn = st.Usage?["prompt_tokens"]?.GetValue<int>() ?? 0;
int nOut = st.Usage?["completion_tokens"]?.GetValue<int>() ?? 0;
Console.WriteLine($"[llm] usage: prompt_tokens = {nIn}, completion_tokens = {nOut}");
string? pIn = Environment.GetEnvironmentVariable("LLM_PRICE_IN");
string? pOut = Environment.GetEnvironmentVariable("LLM_PRICE_OUT");
if (!string.IsNullOrEmpty(pIn) && !string.IsNullOrEmpty(pOut))   // Công thức (8.1): N_in·p_in + N_out·p_out
{
    double cost = (nIn * double.Parse(pIn) + nOut * double.Parse(pOut)) / 1_000_000;
    Console.WriteLine($"[llm] Chi phí ước tính: {nIn} × {pIn} + {nOut} × {pOut} (USD/1 triệu token) = {cost:F8} USD");
}
return 0;
