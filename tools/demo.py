#!/usr/bin/env python3
"""Chạy kịch bản minh họa của một bài và lưu kết quả thực thi thật.

Mỗi thư mục bài có tệp demo.json mô tả các bước, ví dụ:
{
  "setup":     [["javac", "-d", "out", "EchoServer.java", "EchoClient.java"]],
  "processes": [
    {"name": "server", "cmd": ["java", "-cp", "out", "EchoServer", "127.0.0.1", "9000"],
     "background": true, "wait": 1.0},
    {"name": "client", "cmd": ["java", "-cp", "out", "EchoClient", "127.0.0.1", "9000"],
     "stdin": "Xin chào\\n", "timeout": 20}
  ]
}
- setup: các lệnh chuẩn bị (biên dịch...), chạy tuần tự, dừng nếu lỗi.
- processes: tiến trình nền (background) được khởi động trước và dừng ở cuối;
  tiến trình tiền cảnh chạy tới khi kết thúc. Trường "wait" là số giây chờ sau khi khởi động.
Kết quả: ket_qua.txt (toàn bộ đầu ra) và ket_qua.png (ảnh cửa sổ dòng lệnh của từng tiến trình).

Cách dùng:  python3 tools/demo.py <thư_mục_bài>
"""
import json
import os
import shlex
import signal
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))


def run(folder):
    spec = json.load(open(os.path.join(folder, 'demo.json'), encoding='utf-8'))
    env = dict(os.environ, PYTHONUNBUFFERED='1', PYTHONIOENCODING='utf-8',
               DOTNET_CLI_TELEMETRY_OPTOUT='1', DOTNET_NOLOGO='1')
    env.pop('JAVA_TOOL_OPTIONS', None)          # tránh dòng "Picked up JAVA_TOOL_OPTIONS" trong nhật ký
    env.setdefault('LANG', 'C.UTF-8'); env['LC_ALL'] = 'C.UTF-8'
    env.update(spec.get('env', {}))
    for cmd in spec.get('setup', []):
        r = subprocess.run(cmd, cwd=folder, env=env, capture_output=True, text=True)
        if r.returncode != 0:
            print(r.stdout, r.stderr); sys.exit(f'Lỗi bước chuẩn bị: {cmd}')
    logs, bg = {}, []
    for p in spec['processes']:
        name = p['name']
        path = os.path.join(folder, f'.log_{name}.txt')
        f = open(path, 'w', encoding='utf-8')
        f.write('$ ' + ' '.join(shlex.quote(c) for c in p.get('show', p['cmd'])) + '\n'); f.flush()
        logs[name] = path
        if p.get('background'):
            extra = ({'creationflags': subprocess.CREATE_NEW_PROCESS_GROUP} if os.name == 'nt'
                     else {'start_new_session': True})
            proc = subprocess.Popen(p['cmd'], cwd=folder, env=env, stdout=f, stderr=subprocess.STDOUT,
                                    stdin=subprocess.PIPE if 'stdin' in p else subprocess.DEVNULL,
                                    text=True, **extra)
            if 'stdin' in p:
                proc.stdin.write(p['stdin']); proc.stdin.flush()
            bg.append((proc, f))
            time.sleep(p.get('wait', 1.0))
        else:
            try:
                subprocess.run(p['cmd'], cwd=folder, env=env, stdout=f, stderr=subprocess.STDOUT,
                               input=p.get('stdin', ''), text=True, timeout=p.get('timeout', 30))
            except subprocess.TimeoutExpired:
                f.write('[demo] Hết thời gian chờ, dừng tiến trình\n')
            f.close()
            time.sleep(p.get('wait', 0.3))
    for proc, f in bg:
        try:                                       # dừng như khi nhấn Ctrl+C
            if os.name == 'nt':
                proc.send_signal(signal.CTRL_BREAK_EVENT)
            else:
                os.killpg(proc.pid, signal.SIGINT)
            proc.wait(timeout=3)
        except Exception:
            proc.kill()
        f.close()
    with open(os.path.join(folder, 'ket_qua.txt'), 'w', encoding='utf-8') as out:
        for name, path in logs.items():
            out.write(f'===== {name} =====\n' + open(path, encoding='utf-8').read() + '\n')
    titles = spec.get('titles', {})
    args = [f"{titles.get(n, n)}:{logs[n]}" for n in logs]
    subprocess.run([sys.executable, os.path.join(HERE, 'render_output.py'), '-o',
                    os.path.join(folder, 'ket_qua.png')] + args, check=True)
    for path in logs.values():
        os.remove(path)
    print(open(os.path.join(folder, 'ket_qua.txt'), encoding='utf-8').read())


if __name__ == '__main__':
    run(sys.argv[1] if len(sys.argv) > 1 else '.')
