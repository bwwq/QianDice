from qianbian import serve


def command(request, host):
    if request.get('event') == 'timer':
        state = host.call('storage.get', key='timer_count')
        host.call('storage.put', key='timer_count', value=(state['value'] or 0) + 1, revision=state['revision'])
        return {'public': '定时任务已执行。'}
    if request['command'] == 'pylater':
        host.call('schedule.put', id='example', delay_seconds=1, payload={'purpose': 'example'})
        return {'public': '定时任务已登记。'}
    if request['command'] == 'pytimers':
        state = host.call('storage.get', key='timer_count')
        return {'public': str(state['value'] or 0)}
    user = request['context']['user']
    state = host.call('storage.get', key='greetings')
    count = (state['value'] or 0) + 1
    host.call('storage.put', key='greetings', value=count, revision=state['revision'])
    roll = host.call('dice.roll', expression='1d6')
    return {'public': f'你好，{user}！这是第 {count} 次问好，骰点 {roll["total"]}。'}


serve(command)
