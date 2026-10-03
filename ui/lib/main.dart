import 'dart:async';
import 'dart:convert';
import 'package:flutter/foundation.dart';
import 'package:fluent_ui/fluent_ui.dart';
import 'package:http/http.dart' as http;
import 'platform_web.dart'
    if (dart.library.io) 'platform_native.dart'
    as platform;
import 'web_client_stub.dart'
    if (dart.library.js_interop) 'web_client.dart'
    as client;

Future<void> main(List<String> args) async {
  WidgetsFlutterBinding.ensureInitialized();
  final connection = await platform.connection(args);
  runApp(QianbianApp(api: Api(connection.$1, connection.$2)));
}

class Api {
  Api(String url, this.token) : base = url.isEmpty ? Uri.base.origin : url;
  String base;
  String token;
  final http.Client _client = client.createClient();
  Uri uri(String path) => Uri.parse('$base/api/v1$path');
  Future<Uint8List> download(String path) async {
    final response = await _client.get(
      uri(path),
      headers: {
        if (!kIsWeb && token.isNotEmpty) 'authorization': 'Bearer $token',
      },
    );
    if (response.statusCode != 200)
      throw Exception('导出失败 (${response.statusCode})');
    return response.bodyBytes;
  }

  Future<dynamic> request(
    String path, {
    String method = 'GET',
    Object? body,
  }) async {
    final request = http.Request(method, uri(path));
    request.headers.addAll({
      'content-type': 'application/json',
      'x-qianbian': '1',
      if (!kIsWeb && token.isNotEmpty) 'authorization': 'Bearer $token',
    });
    if (body != null) request.body = jsonEncode(body);
    final response = await http.Response.fromStream(
      await _client.send(request),
    );
    if (response.statusCode == 401) throw Exception('请登录或检查管理令牌');
    dynamic value;
    try {
      value = jsonDecode(response.body);
    } catch (_) {
      value = response.body;
    }
    if (response.statusCode >= 400)
      throw Exception(
        value is Map
            ? value['error'] ?? '请求失败'
            : '请求失败 (${response.statusCode})',
      );
    return value;
  }

  Stream<Map<String, dynamic>> events() async* {
    final request = http.Request('GET', uri('/events'));
    if (!kIsWeb && token.isNotEmpty)
      request.headers['authorization'] = 'Bearer $token';
    final response = await _client.send(request);
    if (response.statusCode != 200) return;
    await for (final line
        in response.stream
            .transform(utf8.decoder)
            .transform(const LineSplitter())) {
      if (line.startsWith('data:')) {
        try {
          yield jsonDecode(line.substring(5).trim()) as Map<String, dynamic>;
        } catch (_) {}
      }
    }
  }
}

IconData glyph(String name) =>
    FluentIcons.allIcons[name] ?? FluentIcons.settings;

Widget field(String label, Widget child, {String? hint}) => Padding(
  padding: const EdgeInsets.only(bottom: 16),
  child: InfoLabel(
    label: label,
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        child,
        if (hint != null)
          Padding(
            padding: const EdgeInsets.only(top: 6),
            child: Text(hint, style: const TextStyle(fontSize: 11)),
          ),
      ],
    ),
  ),
);

Widget commandButton(
  String label,
  VoidCallback? onPressed, {
  String? icon,
  bool primary = false,
}) {
  final content = Row(
    mainAxisSize: MainAxisSize.min,
    children: [
      if (icon != null) ...[
        Icon(glyph(icon), size: 16),
        const SizedBox(width: 8),
      ],
      Text(label),
    ],
  );
  return primary
      ? FilledButton(onPressed: onPressed, child: content)
      : Button(onPressed: onPressed, child: content);
}

class QianbianApp extends StatefulWidget {
  const QianbianApp({super.key, required this.api});
  final Api api;
  @override
  State<QianbianApp> createState() => _QianbianAppState();
}

class _QianbianAppState extends State<QianbianApp> {
  ThemeMode mode = ThemeMode.system;
  bool loggedIn = false;
  @override
  void initState() {
    super.initState();
    _resume();
  }

  Future<void> _resume() async {
    try {
      final status = await widget.api.request('/status');
      if (status['api'] != 1) throw Exception('管理接口版本不兼容');
      if (mounted) setState(() => loggedIn = true);
    } catch (_) {}
  }

  FluentThemeData theme(Brightness brightness) => FluentThemeData(
    brightness: brightness,
    accentColor: Colors.blue,
    fontFamily: 'QianbianSans',
    scaffoldBackgroundColor: brightness == Brightness.dark
        ? const Color(0xff202020)
        : const Color(0xfff3f3f3),
  );
  @override
  Widget build(BuildContext context) => FluentApp(
    title: '千变',
    debugShowCheckedModeBanner: false,
    theme: theme(Brightness.light),
    darkTheme: theme(Brightness.dark),
    themeMode: mode,
    home: loggedIn
        ? Workspace(
            api: widget.api,
            themeMode: mode,
            onTheme: (value) => setState(() => mode = value),
            onLogout: () => setState(() => loggedIn = false),
          )
        : Login(
            api: widget.api,
            onLogin: () => setState(() => loggedIn = true),
          ),
  );
}

class Login extends StatefulWidget {
  const Login({super.key, required this.api, required this.onLogin});
  final Api api;
  final VoidCallback onLogin;
  @override
  State<Login> createState() => _LoginState();
}

class _LoginState extends State<Login> {
  late final address = TextEditingController(text: widget.api.base);
  final secret = TextEditingController();
  bool busy = false;
  String? error;
  @override
  void dispose() {
    address.dispose();
    secret.dispose();
    super.dispose();
  }

  Future<void> submit() async {
    if (busy) return;
    setState(() {
      busy = true;
      error = null;
    });
    try {
      widget.api.base = address.text.trim().replaceAll(RegExp(r'/$'), '');
      widget.api.token = secret.text.trim();
      await widget.api.request(
        '/login',
        method: 'POST',
        body: {'token': widget.api.token},
      );
      final status = await widget.api.request('/status');
      if (status['api'] != 1) throw Exception('管理端与后台版本不兼容');
      widget.onLogin();
    } catch (e) {
      if (mounted) setState(() => error = '$e');
    } finally {
      if (mounted) setState(() => busy = false);
    }
  }

  @override
  Widget build(BuildContext context) => ScaffoldPage(
    content: Center(
      child: SingleChildScrollView(
        padding: const EdgeInsets.all(28),
        child: SizedBox(
          width: 420,
          child: Card(
            padding: const EdgeInsets.all(32),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Icon(
                  glyph('cube_shape'),
                  size: 34,
                  color: FluentTheme.of(context).accentColor,
                ),
                const SizedBox(height: 20),
                Text('千变', style: FluentTheme.of(context).typography.title),
                const SizedBox(height: 28),
                if (!kIsWeb) field('后台地址', TextBox(controller: address)),
                field(
                  '管理令牌',
                  PasswordBox(controller: secret, onSubmitted: (_) => submit()),
                  hint: 'data/config/admin-token.txt',
                ),
                if (error != null) ...[
                  InfoBar(
                    title: const Text('无法登录'),
                    content: Text(error!),
                    severity: InfoBarSeverity.error,
                  ),
                  const SizedBox(height: 16),
                ],
                SizedBox(
                  width: double.infinity,
                  child: FilledButton(
                    onPressed: busy ? null : submit,
                    child: busy
                        ? const SizedBox.square(
                            dimension: 18,
                            child: ProgressRing(strokeWidth: 2),
                          )
                        : const Text('登录'),
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    ),
  );
}

class Workspace extends StatefulWidget {
  const Workspace({
    super.key,
    required this.api,
    required this.themeMode,
    required this.onTheme,
    required this.onLogout,
  });
  final Api api;
  final ThemeMode themeMode;
  final ValueChanged<ThemeMode> onTheme;
  final VoidCallback onLogout;
  @override
  State<Workspace> createState() => _WorkspaceState();
}

class _WorkspaceState extends State<Workspace> {
  int page = 0;
  bool expanded = true, busy = false;
  String? error, feedback;
  Map<String, dynamic> status = {};
  List<dynamic> plugins = [], worlds = [], backups = [];
  StreamSubscription<Map<String, dynamic>>? events;
  Timer? retry;
  final pageContentKey = GlobalKey();
  final pages = [
    ('概览', glyph('home')),
    ('账号与群', glyph('group')),
    ('角色卡', glyph('contact')),
    ('规则', glyph('library')),
    ('牌堆', glyph('album')),
    ('跑团记录', glyph('reading_mode')),
    ('插件', glyph('plug_connected')),
    ('模拟聊天', glyph('chat')),
    ('设置', FluentIcons.settings),
  ];
  @override
  void initState() {
    super.initState();
    refresh();
    subscribe();
  }

  @override
  void dispose() {
    events?.cancel();
    retry?.cancel();
    super.dispose();
  }

  void subscribe() {
    retry?.cancel();
    events?.cancel();
    events = widget.api.events().listen(
      (event) {
        if (!mounted) return;
        setState(() {
          final notices = List<dynamic>.from(status['notices'] ?? []);
          notices.add(event);
          if (notices.length > 100) notices.removeAt(0);
          status['notices'] = notices;
        });
      },
      onError: (_) => reconnect(),
      onDone: reconnect,
    );
  }

  void reconnect() {
    retry?.cancel();
    if (mounted)
      retry = Timer(const Duration(seconds: 5), () {
        refresh();
        subscribe();
      });
  }

  Future<void> refresh() async => action(() async {
    final values = await Future.wait([
      widget.api.request('/status'),
      widget.api.request('/plugins'),
      widget.api.request('/worlds'),
      widget.api.request('/backups'),
    ]);
    if (mounted)
      setState(() {
        status = Map<String, dynamic>.from(values[0]);
        plugins = values[1];
        worlds = values[2];
        backups = values[3];
      });
  });
  Future<void> action(Future<void> Function() run) async {
    if (mounted)
      setState(() {
        busy = true;
        error = null;
      });
    try {
      await run();
    } catch (e) {
      if (mounted) setState(() => error = '$e');
    } finally {
      if (mounted) setState(() => busy = false);
    }
  }

  Future<void> message(String value) async {
    if (mounted) setState(() => feedback = value);
  }

  Future<void> editJson(
    String title,
    dynamic initial,
    Future<void> Function(dynamic) save,
  ) async {
    final control = TextEditingController(
      text: const JsonEncoder.withIndent('  ').convert(initial),
    );
    String? problem;
    bool saving = false;
    await showDialog<void>(
      context: context,
      builder: (ctx) => StatefulBuilder(
        builder: (ctx, set) => ContentDialog(
          constraints: const BoxConstraints(maxWidth: 740),
          title: Text(title),
          content: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              SizedBox(
                height: (MediaQuery.sizeOf(ctx).height * .45)
                    .clamp(120, 350)
                    .toDouble(),
                child: TextBox(
                  controller: control,
                  expands: true,
                  maxLines: null,
                  style: const TextStyle(fontFamily: 'monospace', fontSize: 13),
                ),
              ),
              if (problem != null)
                Padding(
                  padding: const EdgeInsets.only(top: 12),
                  child: InfoBar(
                    title: const Text('无法保存'),
                    content: Text(problem!),
                    severity: InfoBarSeverity.error,
                  ),
                ),
            ],
          ),
          actions: [
            Button(
              onPressed: saving ? null : () => Navigator.pop(ctx),
              child: const Text('取消'),
            ),
            FilledButton(
              onPressed: saving
                  ? null
                  : () async {
                      set(() => saving = true);
                      try {
                        await save(jsonDecode(control.text));
                        if (ctx.mounted) Navigator.pop(ctx);
                      } catch (e) {
                        if (ctx.mounted) set(() => problem = '$e');
                      } finally {
                        if (ctx.mounted) set(() => saving = false);
                      }
                    },
              child: Text(saving ? '保存中…' : '保存'),
            ),
          ],
        ),
      ),
    );
    control.dispose();
  }

  Future<String?> prompt(
    String title, {
    String initial = '',
    String? hint,
  }) async {
    final control = TextEditingController(text: initial);
    final result = await showDialog<String>(
      context: context,
      builder: (ctx) => ContentDialog(
        title: Text(title),
        content: TextBox(
          controller: control,
          autofocus: true,
          placeholder: hint,
          onSubmitted: (value) => Navigator.pop(ctx, value),
        ),
        actions: [
          Button(onPressed: () => Navigator.pop(ctx), child: const Text('取消')),
          FilledButton(
            onPressed: () => Navigator.pop(ctx, control.text),
            child: const Text('确定'),
          ),
        ],
      ),
    );
    control.dispose();
    return result;
  }

  Future<void> configurePlugin(dynamic plugin) async {
    final id = plugin['manifest']['id'];
    final saved = await widget.api.request('/plugin-config/$id');
    final values = Map<String, dynamic>.from(saved['value'] ?? {});
    final props = Map<String, dynamic>.from(
      plugin['manifest']['config_schema']['properties'] ?? {},
    );
    Future<void> save(dynamic value) async {
      await widget.api.request(
        '/plugin-config/$id',
        method: 'PUT',
        body: {'value': value, 'revision': saved['revision']},
      );
    }

    if (props.isEmpty) {
      await editJson('插件配置 · $id', values, save);
      return;
    }
    if (!mounted) return;
    final controls = <String, TextEditingController>{};
    for (final entry in props.entries) {
      values.putIfAbsent(entry.key, () => entry.value['default']);
      if (entry.value['enum'] is List &&
          values[entry.key] == null &&
          (entry.value['enum'] as List).isNotEmpty)
        values[entry.key] = entry.value['enum'][0];
      if (entry.value['type'] != 'boolean' && entry.value['enum'] is! List)
        controls[entry.key] = TextEditingController(
          text: '${values[entry.key] ?? ''}',
        );
    }
    String? problem;
    bool saving = false;
    await showDialog<void>(
      context: context,
      builder: (ctx) => StatefulBuilder(
        builder: (ctx, update) => ContentDialog(
          constraints: const BoxConstraints(maxWidth: 560),
          title: Text('配置 $id'),
          content: SingleChildScrollView(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                for (final entry in props.entries)
                  Builder(
                    builder: (ctx) {
                      final spec = entry.value as Map;
                      final label = '${spec['title'] ?? entry.key}';
                      if (spec['type'] == 'boolean')
                        return Padding(
                          padding: const EdgeInsets.only(bottom: 16),
                          child: ToggleSwitch(
                            content: Text(label),
                            checked: values[entry.key] == true,
                            onChanged: (value) =>
                                update(() => values[entry.key] = value),
                          ),
                        );
                      if (spec['enum'] is List)
                        return field(
                          label,
                          ComboBox<dynamic>(
                            value: values[entry.key],
                            items: (spec['enum'] as List)
                                .map(
                                  (value) => ComboBoxItem<dynamic>(
                                    value: value,
                                    child: Text('$value'),
                                  ),
                                )
                                .toList(),
                            onChanged: (value) =>
                                update(() => values[entry.key] = value),
                          ),
                        );
                      return field(
                        label,
                        TextBox(
                          controller: controls[entry.key],
                          keyboardType:
                              ['integer', 'number'].contains(spec['type'])
                              ? TextInputType.number
                              : TextInputType.text,
                          onChanged: (value) =>
                              values[entry.key] = spec['type'] == 'integer'
                              ? int.tryParse(value)
                              : spec['type'] == 'number'
                              ? double.tryParse(value)
                              : value,
                        ),
                        hint: spec['description'] as String?,
                      );
                    },
                  ),
                if (problem != null)
                  InfoBar(
                    title: const Text('无法保存'),
                    content: Text(problem!),
                    severity: InfoBarSeverity.error,
                  ),
              ],
            ),
          ),
          actions: [
            Button(
              onPressed: saving ? null : () => Navigator.pop(ctx),
              child: const Text('取消'),
            ),
            FilledButton(
              onPressed: saving
                  ? null
                  : () async {
                      update(() => saving = true);
                      try {
                        await save(values);
                        if (ctx.mounted) Navigator.pop(ctx);
                      } catch (e) {
                        if (ctx.mounted) update(() => problem = '$e');
                      } finally {
                        if (ctx.mounted) update(() => saving = false);
                      }
                    },
              child: Text(saving ? '保存中…' : '保存'),
            ),
          ],
        ),
      ),
    );
    for (final control in controls.values) {
      control.dispose();
    }
  }

  Future<bool> confirm(String title, String detail) async =>
      await showDialog<bool>(
        context: context,
        builder: (ctx) => ContentDialog(
          title: Text(title),
          content: Text(detail),
          actions: [
            Button(
              onPressed: () => Navigator.pop(ctx, false),
              child: const Text('取消'),
            ),
            FilledButton(
              onPressed: () => Navigator.pop(ctx, true),
              child: const Text('确定'),
            ),
          ],
        ),
      ) ??
      false;

  @override
  Widget build(BuildContext context) => NavigationView(
    titleBar: SizedBox(
      height: 54,
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 20),
        child: Row(
          children: [
            Icon(
              glyph('cube_shape'),
              size: 20,
              color: FluentTheme.of(context).accentColor,
            ),
            const SizedBox(width: 10),
            const Text('千变'),
            const Spacer(),
            if (busy)
              const SizedBox.square(
                dimension: 18,
                child: ProgressRing(strokeWidth: 2),
              ),
            const SizedBox(width: 12),
            Tooltip(
              message: '刷新',
              child: IconButton(
                onPressed: busy ? null : refresh,
                icon: Icon(glyph('refresh'), size: 16),
              ),
            ),
            Tooltip(
              message: '切换主题',
              child: IconButton(
                icon: Icon(glyph('brightness'), size: 16),
                onPressed: () => widget.onTheme(
                  FluentTheme.of(context).brightness == Brightness.dark
                      ? ThemeMode.light
                      : ThemeMode.dark,
                ),
              ),
            ),
            Tooltip(
              message: expanded ? '收起侧栏' : '展开侧栏',
              child: IconButton(
                icon: Icon(glyph('global_nav_button'), size: 16),
                onPressed: () => setState(() => expanded = !expanded),
              ),
            ),
          ],
        ),
      ),
    ),
    pane: NavigationPane(
      selected: page,
      onChanged: (index) => setState(() => page = index),
      displayMode: MediaQuery.sizeOf(context).width < 760
          ? PaneDisplayMode.minimal
          : expanded
          ? PaneDisplayMode.expanded
          : PaneDisplayMode.compact,
      header: Padding(
        padding: const EdgeInsets.fromLTRB(18, 18, 18, 12),
        child: Text('工作台', style: FluentTheme.of(context).typography.caption),
      ),
      items: [
        for (final entry in pages)
          PaneItem(
            icon: Icon(entry.$2, size: 18),
            title: Text(entry.$1),
            body: const SizedBox.shrink(),
          ),
      ],
    ),
    paneBodyBuilder: (item, selectedBody) => ScaffoldPage(
      header: PageHeader(title: Text(pages[page].$1)),
      content: Column(
        children: [
          if (error != null)
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: 24, vertical: 8),
              child: InfoBar(
                title: const Text('操作未完成'),
                content: Text(error!),
                severity: InfoBarSeverity.error,
                onClose: () => setState(() => error = null),
              ),
            ),
          if (feedback != null)
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: 24, vertical: 8),
              child: InfoBar(
                title: Text(feedback!),
                severity: InfoBarSeverity.success,
                onClose: () => setState(() => feedback = null),
              ),
            ),
          Expanded(
            child: IndexedStack(
              key: pageContentKey,
              index: page,
              children: [
                overview(),
                accounts(),
                characters(),
                ContentPage(
                  api: widget.api,
                  kind: 'rules',
                  edit: editJson,
                  prompt: prompt,
                ),
                ContentPage(
                  api: widget.api,
                  kind: 'decks',
                  edit: editJson,
                  prompt: prompt,
                ),
                LogsPage(api: widget.api, worlds: worlds),
                pluginPage(),
                ChatPage(api: widget.api),
                settings(),
              ],
            ),
          ),
        ],
      ),
    ),
  );
  Widget body(List<Widget> children) => ListView(
    padding: const EdgeInsets.fromLTRB(24, 8, 24, 24),
    children: children,
  );
  Widget section(String title, List<Widget> children, {Widget? action}) =>
      Padding(
        padding: const EdgeInsets.only(bottom: 20),
        child: Card(
          padding: const EdgeInsets.all(22),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(
                children: [
                  Expanded(
                    child: Text(
                      title,
                      style: FluentTheme.of(context).typography.subtitle,
                    ),
                  ),
                  if (action != null) action,
                ],
              ),
              const SizedBox(height: 18),
              ...children,
            ],
          ),
        ),
      );
  Widget overview() {
    final connections = status['connections'] as List? ?? [];
    final notices = status['notices'] as List? ?? [];
    final cardCount = worlds.fold<int>(
      0,
      (sum, world) =>
          sum +
          ((world['value']['players'] as Map?) ?? {}).values.fold<int>(
            0,
            (count, player) => count + ((player['cards'] as Map?) ?? {}).length,
          ),
    );
    return body([
      section('下一场冒险', [
        Wrap(
          spacing: 12,
          runSpacing: 12,
          children: [
            commandButton(
              '模拟掷骰',
              () => setState(() => page = 7),
              icon: 'game',
              primary: true,
            ),
            commandButton(
              '管理账号',
              () => setState(() => page = 1),
              icon: 'group',
            ),
          ],
        ),
      ]),
      Wrap(
        spacing: 16,
        runSpacing: 16,
        children:
            [
                  ('在线账号', '${connections.length}', 'group'),
                  ('角色卡', '$cardCount', 'contact'),
                  (
                    '启用插件',
                    '${plugins.where((plugin) => plugin['enabled'] == true).length}',
                    'plug_connected',
                  ),
                  (
                    '运行时间',
                    '${((status['uptime_seconds'] ?? 0) / 60).floor()} 分钟',
                    'clock',
                  ),
                ]
                .map(
                  (entry) => SizedBox(
                    width: 196,
                    child: Card(
                      padding: const EdgeInsets.all(20),
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Row(
                            children: [
                              Expanded(
                                child: Text(
                                  entry.$1,
                                  style: FluentTheme.of(
                                    context,
                                  ).typography.caption,
                                ),
                              ),
                              Icon(glyph(entry.$3), size: 18),
                            ],
                          ),
                          const SizedBox(height: 18),
                          Text(
                            entry.$2,
                            style: FluentTheme.of(context).typography.title,
                          ),
                        ],
                      ),
                    ),
                  ),
                )
                .toList(),
      ),
      const SizedBox(height: 24),
      section(
        '最近动态',
        notices.isEmpty
            ? [
                const Padding(
                  padding: EdgeInsets.symmetric(vertical: 28),
                  child: Text('暂无动态'),
                ),
              ]
            : notices.reversed
                  .take(30)
                  .map<Widget>(
                    (notice) => ListTile(
                      leading: Icon(
                        glyph(
                          notice['level'] == 'warning' ? 'info' : 'circle_fill',
                        ),
                        size: 12,
                      ),
                      title: Text('${notice['message']}'),
                      subtitle: Text('${notice['time']}'),
                    ),
                  )
                  .toList(),
      ),
    ]);
  }

  Widget accounts() => body([
    section('机器人连接', [
      commandButton(
        '编辑账号',
        () => action(() async {
          final config = await widget.api.request('/config');
          await editJson('账号与群配置', config, (value) async {
            final result = await widget.api.request(
              '/config',
              method: 'PUT',
              body: value,
            );
            await message(result['message']);
          });
        }),
        icon: 'edit',
        primary: true,
      ),
      const SizedBox(height: 12),
      const Text(
        'OneBot 11 · 反向连接使用 Universal，连接配置需重启后台。',
        style: TextStyle(fontSize: 11),
      ),
      for (final id in (status['connections'] as List?) ?? [])
        ListTile(
          leading: Icon(glyph('check_mark')),
          title: Text('$id'),
          subtitle: const Text('已连接'),
        ),
    ]),
    section('会话', [
      for (final world in worlds)
        for (final entry in ((world['value']['rooms'] as Map?) ?? {}).entries)
          ListTile(
            title: Text('${entry.key}'),
            subtitle: Text('${entry.value['rule']}'),
            trailing: ToggleSwitch(
              checked: entry.value['enabled'] == true,
              onChanged: (value) => action(() async {
                final current = await widget.api.request(
                  '/world?scope=${Uri.encodeComponent('${world['key']}')}',
                );
                current['value']['rooms'][entry.key]['enabled'] = value;
                await widget.api.request(
                  '/world',
                  method: 'PUT',
                  body: {
                    'scope': world['key'],
                    'revision': current['revision'],
                    'value': current['value'],
                  },
                );
                await refresh();
              }),
            ),
          ),
      if (worlds.every(
        (world) => ((world['value']['rooms'] as Map?) ?? {}).isEmpty,
      ))
        const Text('暂无会话'),
    ]),
  ]);
  Widget characters() {
    final cards = <Widget>[];
    for (final world in worlds)
      for (final player
          in ((world['value']['players'] as Map?) ?? {}).entries) {
        for (final entry in ((player.value['cards'] as Map?) ?? {}).entries) {
          cards.add(
            Padding(
              padding: const EdgeInsets.only(bottom: 12),
              child: Card(
                child: ListTile(
                  leading: Icon(glyph('contact')),
                  title: Text('${entry.key}'),
                  subtitle: Text(
                    '${player.key} · ${entry.value['rule']}\n${(entry.value['attrs'] as Map).entries.take(5).map((attr) => '${attr.key} ${attr.value}').join('  ')}',
                  ),
                  trailing: Tooltip(
                    message: '编辑角色',
                    child: IconButton(
                      icon: Icon(glyph('edit'), size: 16),
                      onPressed: () => editJson(
                        '编辑 ${entry.key}',
                        entry.value,
                        (value) async {
                          player.value['cards'][entry.key] = value;
                          await widget.api.request(
                            '/world',
                            method: 'PUT',
                            body: {
                              'scope': world['key'],
                              'revision': world['revision'],
                              'value': world['value'],
                            },
                          );
                          await refresh();
                        },
                      ),
                    ),
                  ),
                ),
              ),
            ),
          );
        }
      }
    return body([
      if (cards.isEmpty)
        section('暂无角色', [const Text('.st new 名称 创建角色 · .st lock 绑定当前群')]),
      ...cards,
    ]);
  }

  Widget pluginPage() => body([
    section('插件', [
      commandButton(
        '加载或更新',
        () async {
          final path = await prompt('加载插件', hint: '插件ID/版本/plugin.json');
          if (path == null || path.isEmpty) return;
          await action(() async {
            await widget.api.request(
              '/plugins/load',
              method: 'POST',
              body: {'path': path},
            );
            await refresh();
          });
        },
        icon: 'add',
        primary: true,
      ),
    ]),
    ...plugins.map(
      (plugin) => Padding(
        padding: const EdgeInsets.only(bottom: 12),
        child: Card(
          padding: const EdgeInsets.all(20),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(
                children: [
                  Icon(glyph('plug_connected'), size: 20),
                  const SizedBox(width: 14),
                  Expanded(
                    child: Text(
                      '${plugin['manifest']['id']}',
                      style: FluentTheme.of(context).typography.subtitle,
                    ),
                  ),
                  Text('v${plugin['manifest']['version']}'),
                  const SizedBox(width: 12),
                  Tooltip(
                    message: '配置',
                    child: IconButton(
                      onPressed: () => action(() => configurePlugin(plugin)),
                      icon: Icon(glyph('settings'), size: 16),
                    ),
                  ),
                  const SizedBox(width: 12),
                  ToggleSwitch(
                    checked: plugin['enabled'] == true,
                    onChanged: (value) => action(() async {
                      await widget.api.request(
                        '/plugins/${plugin['manifest']['id']}/${value ? 'enable' : 'disable'}',
                        method: 'POST',
                      );
                      await refresh();
                    }),
                  ),
                ],
              ),
              const SizedBox(height: 12),
              Text(
                (plugin['manifest']['commands'] as List)
                    .map((command) => '.$command')
                    .join('  '),
                style: FluentTheme.of(context).typography.caption,
              ),
              if (plugin['error'] != null)
                Padding(
                  padding: const EdgeInsets.only(top: 12),
                  child: InfoBar(
                    title: const Text('插件异常'),
                    content: Text('${plugin['error']}'),
                    severity: InfoBarSeverity.error,
                  ),
                ),
            ],
          ),
        ),
      ),
    ),
  ]);
  Widget settings() => body([
    section('外观', [
      field(
        '主题',
        ComboBox<ThemeMode>(
          value: widget.themeMode,
          onChanged: (mode) {
            if (mode != null) widget.onTheme(mode);
          },
          items: const [
            ComboBoxItem(value: ThemeMode.system, child: Text('跟随系统')),
            ComboBoxItem(value: ThemeMode.light, child: Text('浅色')),
            ComboBoxItem(value: ThemeMode.dark, child: Text('深色')),
          ],
        ),
      ),
    ]),
    section('数据与备份', [
      Text(
        '${status['data_directory'] ?? ''}',
        style: FluentTheme.of(context).typography.caption,
      ),
      const SizedBox(height: 16),
      commandButton(
        '创建完整备份',
        () => action(() async {
          final result = await widget.api.request('/backups', method: 'POST');
          await message('备份完成：${result['id']}');
          await refresh();
        }),
        icon: 'save',
      ),
      const SizedBox(height: 16),
      for (final backup in backups.reversed.take(10))
        ListTile(leading: Icon(glyph('archive')), title: Text('$backup')),
      const SizedBox(height: 12),
      const Text('恢复前停止后台。恢复会回到备份时点，并另存当前数据。', style: TextStyle(fontSize: 11)),
    ]),
    section('运行', [
      Text(
        '千变 ${status['version'] ?? ''} · ${status['platform'] ?? ''}',
        style: FluentTheme.of(context).typography.caption,
      ),
      const SizedBox(height: 16),
      Wrap(
        spacing: 12,
        runSpacing: 12,
        children: [
          commandButton('退出登录', () async {
            await widget.api.request('/logout', method: 'POST');
            widget.api.token = '';
            widget.onLogout();
          }, icon: 'sign_out'),
          commandButton('停止后台', () async {
            if (await confirm('停止千变后台？', '机器人连接与插件都会停止。关闭管理窗口不会停止后台。'))
              await action(() async {
                await widget.api.request('/shutdown', method: 'POST');
                await message('后台正在停止');
              });
          }, icon: 'power_button'),
        ],
      ),
    ]),
  ]);
}

class ChatPage extends StatefulWidget {
  const ChatPage({super.key, required this.api});
  final Api api;
  @override
  State<ChatPage> createState() => _ChatPageState();
}

class _ChatPageState extends State<ChatPage> {
  final input = TextEditingController(),
      user = TextEditingController(text: '调查员'),
      group = TextEditingController(text: '试验团');
  final List<(bool, String)> messages = [];
  bool busy = false;
  @override
  void dispose() {
    input.dispose();
    user.dispose();
    group.dispose();
    super.dispose();
  }

  Future<void> send() async {
    final text = input.text.trim();
    if (text.isEmpty || busy) return;
    input.clear();
    setState(() {
      busy = true;
      messages.add((true, text));
    });
    try {
      final result = await widget.api.request(
        '/simulate',
        method: 'POST',
        body: {
          'context': {
            'platform': 'simulation',
            'account': 'sandbox',
            'user': user.text,
            'group': group.text,
            'name': user.text,
          },
          'text': text,
        },
      );
      if (mounted)
        setState(
          () => messages.add((
            false,
            '${result['public']}${result['private'] == null ? '' : '\n【仅自己可见】${result['private']}'}',
          )),
        );
    } catch (e) {
      if (mounted) setState(() => messages.add((false, '$e')));
    } finally {
      if (mounted)
        setState(() {
          busy = false;
          if (messages.length > 200)
            messages.removeRange(0, messages.length - 200);
        });
    }
  }

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.fromLTRB(24, 8, 24, 24),
    child: Column(
      children: [
        Row(
          children: [
            Expanded(child: field('玩家', TextBox(controller: user))),
            const SizedBox(width: 16),
            Expanded(child: field('练习群', TextBox(controller: group))),
          ],
        ),
        const InfoBar(title: Text('练习模式'), content: Text('不发送 QQ 消息，不修改正式角色。')),
        const SizedBox(height: 16),
        Expanded(
          child: ListView.builder(
            reverse: true,
            itemCount: messages.length,
            itemBuilder: (context, index) {
              final item = messages[messages.length - 1 - index];
              return Align(
                alignment: item.$1
                    ? Alignment.centerRight
                    : Alignment.centerLeft,
                child: Container(
                  constraints: const BoxConstraints(maxWidth: 760),
                  margin: const EdgeInsets.only(bottom: 12),
                  padding: const EdgeInsets.all(16),
                  decoration: BoxDecoration(
                    color: item.$1
                        ? FluentTheme.of(
                            context,
                          ).accentColor.withValues(alpha: .13)
                        : FluentTheme.of(context).brightness == Brightness.dark
                        ? const Color(0xff2b2b2b)
                        : const Color(0xffffffff),
                    borderRadius: BorderRadius.circular(8),
                  ),
                  child: Text(item.$2),
                ),
              );
            },
          ),
        ),
        const SizedBox(height: 12),
        Row(
          children: [
            Expanded(
              child: TextBox(
                controller: input,
                onSubmitted: (_) => send(),
                placeholder: '输入指令，如 .r 1d100',
              ),
            ),
            const SizedBox(width: 12),
            FilledButton(
              onPressed: busy ? null : send,
              child: busy
                  ? const SizedBox.square(
                      dimension: 16,
                      child: ProgressRing(strokeWidth: 2),
                    )
                  : const Text('发送'),
            ),
          ],
        ),
      ],
    ),
  );
}

typedef JsonEditor =
    Future<void> Function(String, dynamic, Future<void> Function(dynamic));
typedef Prompt =
    Future<String?> Function(String, {String initial, String? hint});

class ContentPage extends StatefulWidget {
  const ContentPage({
    super.key,
    required this.api,
    required this.kind,
    required this.edit,
    required this.prompt,
  });
  final Api api;
  final String kind;
  final JsonEditor edit;
  final Prompt prompt;
  @override
  State<ContentPage> createState() => _ContentPageState();
}

class _ContentPageState extends State<ContentPage> {
  List<dynamic> names = [];
  String? error;
  @override
  void initState() {
    super.initState();
    load();
  }

  Future<void> load() async {
    try {
      final value = await widget.api.request('/content/${widget.kind}');
      if (mounted)
        setState(() {
          names = value;
          error = null;
        });
    } catch (e) {
      if (mounted) setState(() => error = '$e');
    }
  }

  Future<void> edit(String name, {bool create = false}) async {
    try {
      final value = create
          ? (widget.kind == 'decks'
                ? {
                    'without_replacement': false,
                    'entries': [
                      {'text': '新的故事开始了。', 'weight': 1},
                    ],
                  }
                : {
                    'id': name.replaceAll('.json', ''),
                    'label': '自定义规则',
                    'faces': 100,
                    'comparison': 'lte',
                    'critical': 1,
                    'fumble': 100,
                  })
          : await widget.api.request(
              '/content/${widget.kind}/${Uri.encodeComponent(name)}',
            );
      await widget.edit(name, value, (updated) async {
        await widget.api.request(
          '/content/${widget.kind}/${Uri.encodeComponent(name)}',
          method: 'PUT',
          body: updated,
        );
        await load();
      });
    } catch (e) {
      if (mounted) setState(() => error = '$e');
    }
  }

  @override
  Widget build(BuildContext context) => ListView(
    padding: const EdgeInsets.fromLTRB(24, 8, 24, 24),
    children: [
      Row(
        children: [
          commandButton(
            '新建',
            () async {
              final name = await widget.prompt(
                '新建${widget.kind == 'decks' ? '牌堆' : '规则'}',
                hint: '名称.json',
              );
              if (name == null || name.isEmpty) return;
              await edit(
                name.endsWith('.json') ? name : '$name.json',
                create: true,
              );
            },
            icon: 'add',
            primary: true,
          ),
          const Spacer(),
          Tooltip(
            message: '刷新',
            child: IconButton(
              onPressed: load,
              icon: Icon(glyph('refresh'), size: 16),
            ),
          ),
        ],
      ),
      const SizedBox(height: 20),
      if (error != null)
        InfoBar(
          title: const Text('操作未完成'),
          content: Text(error!),
          severity: InfoBarSeverity.error,
        ),
      for (final name in names)
        Padding(
          padding: const EdgeInsets.only(bottom: 10),
          child: Card(
            child: ListTile(
              leading: Icon(
                glyph(widget.kind == 'decks' ? 'album' : 'library'),
                size: 20,
              ),
              title: Text('$name'),
              trailing: Icon(glyph('chevron_right'), size: 12),
              onPressed: () => edit('$name'),
            ),
          ),
        ),
      if (names.isEmpty)
        const Padding(
          padding: EdgeInsets.all(40),
          child: Center(child: Text('暂无自定义内容')),
        ),
    ],
  );
}

class LogsPage extends StatefulWidget {
  const LogsPage({super.key, required this.api, required this.worlds});
  final Api api;
  final List<dynamic> worlds;
  @override
  State<LogsPage> createState() => _LogsPageState();
}

class _LogsPageState extends State<LogsPage> {
  final scope = TextEditingController(), session = TextEditingController();
  List<dynamic> rows = [], sessions = [];
  int? selected;
  String? error, notice;
  @override
  void initState() {
    super.initState();
    loadSessions();
  }

  Future<void> loadSessions() async {
    try {
      final value = await widget.api.request('/sessions');
      if (mounted)
        setState(() {
          sessions = value;
          if (selected != null && selected! >= sessions.length) selected = null;
        });
    } catch (e) {
      if (mounted) setState(() => error = '$e');
    }
  }

  @override
  void dispose() {
    scope.dispose();
    session.dispose();
    super.dispose();
  }

  String query([String format = 'txt']) => Uri(
    queryParameters: {
      'scope': scope.text,
      'session': session.text,
      'format': format,
    },
  ).query;
  Future<void> load({bool more = false}) async {
    try {
      final result = await widget.api.request(
        '/logs?${query()}&after=${more && rows.isNotEmpty ? rows.last['id'] : 0}',
      );
      if (mounted)
        setState(() {
          rows = more
              ? [
                  ...rows.skip(rows.length > 800 ? rows.length - 800 : 0),
                  ...result,
                ]
              : result;
          error = null;
        });
    } catch (e) {
      if (mounted) setState(() => error = '$e');
    }
  }

  @override
  Widget build(BuildContext context) => ListView(
    padding: const EdgeInsets.fromLTRB(24, 8, 24, 24),
    children: [
      Row(
        children: [
          Expanded(
            child: field(
              '历史团录',
              ComboBox<int>(
                isExpanded: true,
                value: selected,
                placeholder: const Text('选择团录'),
                items: [
                  for (var index = 0; index < sessions.length; index++)
                    ComboBoxItem(
                      value: index,
                      child: Text(
                        '${sessions[index]['session']} · ${sessions[index]['count']} 条',
                      ),
                    ),
                ],
                onChanged: (index) {
                  if (index == null) return;
                  setState(() => selected = index);
                  scope.text = '${sessions[index]['scope']}';
                  session.text = '${sessions[index]['session']}';
                  load();
                },
              ),
            ),
          ),
          const SizedBox(width: 12),
          Tooltip(
            message: '刷新',
            child: IconButton(
              onPressed: loadSessions,
              icon: Icon(glyph('refresh'), size: 16),
            ),
          ),
        ],
      ),
      field(
        '账号数据',
        ComboBox<String>(
          isExpanded: true,
          value: widget.worlds.any((world) => '${world['key']}' == scope.text)
              ? scope.text
              : null,
          items: widget.worlds
              .map<ComboBoxItem<String>>(
                (world) => ComboBoxItem(
                  value: '${world['key']}',
                  child: Text('${world['key']}'),
                ),
              )
              .toList(),
          onChanged: (value) => setState(() => scope.text = value ?? ''),
        ),
      ),
      field('日志名称', TextBox(controller: session, placeholder: '.log on 使用的名称')),
      Wrap(
        spacing: 12,
        runSpacing: 12,
        children: [
          commandButton('查看记录', load, primary: true),
          for (final format in ['txt', 'html', 'json'])
            commandButton('导出 ${format.toUpperCase()}', () async {
              try {
                final bytes = await widget.api.download(
                  '/export?${query(format)}',
                );
                final saved = await platform.save(
                  bytes,
                  'qianbian-log.$format',
                  format == 'html'
                      ? 'text/html'
                      : format == 'json'
                      ? 'application/json'
                      : 'text/plain',
                );
                if (mounted && saved) setState(() => notice = '团录已导出');
              } catch (e) {
                if (mounted) setState(() => error = '$e');
              }
            }, icon: 'download'),
        ],
      ),
      const SizedBox(height: 20),
      if (error != null)
        InfoBar(
          title: const Text('操作未完成'),
          content: Text(error!),
          severity: InfoBarSeverity.error,
          onClose: () => setState(() => error = null),
        ),
      if (notice != null)
        InfoBar(
          title: Text(notice!),
          severity: InfoBarSeverity.success,
          onClose: () => setState(() => notice = null),
        ),
      for (final row in rows)
        ListTile(
          title: Text('${row['text']}'),
          subtitle: Text('${row['actor']} · ${row['time']}'),
        ),
      if (rows.isNotEmpty)
        Button(onPressed: () => load(more: true), child: const Text('加载更多')),
    ],
  );
}
