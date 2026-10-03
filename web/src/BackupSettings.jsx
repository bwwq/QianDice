import React, {useEffect, useState} from 'react';

const parts = [['game', '角色与团录'], ['config', '配置'], ['plugins', '插件与数据'], ['rules', '规则'], ['decks', '牌堆'], ['logs', '日志文件']];
const complete = Object.fromEntries(parts.map(([key]) => [key, true]));
export default function BackupSettings({api, act, notify, refresh, busy}) {
  const [config, setConfig] = useState(null);
  const [status, setStatus] = useState({});
  const [error, setError] = useState('');
  const [dirty, setDirty] = useState(false);
  useEffect(() => {
    let alive = true;
    api('/config').then(value => {if (alive) setConfig(value.backup);}).catch(e => {if (alive) setError(e.message);});
    const update = () => api('/backups/status').then(value => {if (alive) setStatus(value);}).catch(() => {});
    update(); const timer = setInterval(update, 10000);
    return () => {alive = false; clearInterval(timer);};
  }, []);
  const change = patch => {setConfig(old => ({...old, ...patch})); setDirty(true);};
  const remote = (name, key, value) => change({[name]: {...config[name], [key]: value}});
  const input = (name, key, label, secret = false, placeholder = '') => <label className="field"><span>{label}</span><input type={secret ? 'password' : 'text'} autoComplete={secret ? 'new-password' : 'off'} value={config[name][key]} placeholder={placeholder} onChange={e => remote(name, key, e.target.value)}/></label>;
  const save = async () => {await api('/config', 'PUT', {backup: config}); setConfig((await api('/config')).backup); setDirty(false); await refresh(); notify('备份设置已保存');};
  const create = contents => act(async () => {
    const result = await api('/backups', 'POST', {contents});
    setStatus(await api('/backups/status')); await refresh();
    notify(result.status === 'partial' ? '本地备份已保存，部分上传失败' : '备份完成');
  });
  if (!config) return <p className={error ? 'error' : 'muted'}>{error || '正在读取备份设置'}</p>;
  const stamp = seconds => seconds ? new Date(seconds * 1000).toLocaleString() : '—';
  return <div className="backup-settings">
    <form onSubmit={e => {e.preventDefault(); act(save);}}>
      <div className="row spread"><h3>自动备份</h3><label className="switch-label"><input type="checkbox" className="switch" checked={config.enabled} onChange={e => change({enabled: e.target.checked})}/>启用</label></div>
      <div className="form-grid backup-spacing"><label className="field"><span>间隔（分钟）</span><input type="number" min="1" max="525600" required value={config.interval_minutes} onChange={e => change({interval_minutes: Number(e.target.value)})}/></label><label className="field"><span>保留自动备份（次）</span><input type="number" min="1" max="1000" required value={config.keep} onChange={e => change({keep: Number(e.target.value)})}/></label></div>
      <div className="row spread"><h3>备份内容</h3><button type="button" className="button text" onClick={() => change({contents: {...complete}})}>全选</button></div>
      <div className="backup-options">{parts.map(([key, label]) => <label className="check" key={key}><input type="checkbox" checked={config.contents[key]} onChange={e => change({contents: {...config.contents, [key]: e.target.checked}})}/>{label}</label>)}</div>
      <div className="backup-destination"><div className="row spread"><h3>WebDAV</h3><label className="switch-label"><input className="switch" type="checkbox" checked={config.webdav.enabled} onChange={e => remote('webdav', 'enabled', e.target.checked)}/>备份后上传</label></div>{config.webdav.enabled && <div className="form-grid backup-spacing">{input('webdav', 'url', '备份目录地址', false, 'https://example.com/dav/qianbian/')}{input('webdav', 'username', '用户名')}{input('webdav', 'password', '密码', true, config.webdav.has_password ? '已保存，留空保持' : '')}</div>}</div>
      <div className="backup-destination"><div className="row spread"><h3>S3</h3><label className="switch-label"><input className="switch" type="checkbox" checked={config.s3.enabled} onChange={e => remote('s3', 'enabled', e.target.checked)}/>备份后上传</label></div>{config.s3.enabled && <><div className="form-grid backup-spacing">{input('s3', 'endpoint', '服务地址', false, 'https://s3.us-east-1.amazonaws.com')}{input('s3', 'region', '区域')}{input('s3', 'bucket', '存储桶')}{input('s3', 'prefix', '目录前缀')}{input('s3', 'access_key', 'Access Key')}{input('s3', 'secret_key', 'Secret Key', true, config.s3.has_secret_key ? '已保存，留空保持' : '')}{input('s3', 'session_token', '会话令牌（可选）', true, config.s3.has_session_token ? '已保存，留空保持' : '')}</div><label className="check"><input type="checkbox" checked={config.s3.path_style} onChange={e => remote('s3', 'path_style', e.target.checked)}/>使用路径式地址</label></>}</div>
      <div className="row backup-spacing"><button className="button primary" disabled={busy || status.running || !dirty}>保存备份设置</button><button type="button" className="button" disabled={busy || status.running} onClick={() => create(config.contents)}>立即备份</button><button type="button" className="button text" disabled={busy || status.running} onClick={() => create(complete)}>完整备份</button></div>
      <p className="muted backup-caption">手动备份单独保留。上传选项保存后生效。</p>
    </form>
    <div className="backup-status"><span>{status.running ? '正在备份' : config.enabled ? `下次备份 ${stamp(status.next_at)}` : '自动备份已关闭'}</span>{status.last_result?.id && <span>最近备份 {status.last_result.id}</span>}{status.last_result?.status === 'failed' && <span className="error">{status.last_result.message}</span>}{status.last_result?.uploads?.map(item => <span key={item.target} className={item.status === 'failed' ? 'error' : 'muted'}>{item.target === 'webdav' ? 'WebDAV' : item.target === 's3' ? 'S3' : '备份打包'} · {item.status === 'success' ? '已上传' : item.message}</span>)}{status.last_result?.warnings?.map((message, i) => <span className="error" key={i}>{message}</span>)}</div>
  </div>;
}
