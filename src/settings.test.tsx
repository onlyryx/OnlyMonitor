import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { useState } from 'react';
import { afterEach, expect, it, vi } from 'vitest';
import { SettingsPanel } from './SettingsPanel';
import { monitorState } from './testFixtures';

afterEach(cleanup);
it('only confirms successful saves and preserves draft and effective values after failure', async () => {
  const save = vi.fn().mockRejectedValueOnce(new Error('无法保存设置')).mockResolvedValue(undefined);
  const settings = monitorState().settings;
  function Harness() {
    const [effective, setEffective] = useState(settings);
    const [draft, setDraft] = useState(settings);
    return <SettingsPanel settings={effective} draft={draft} onDraftChange={setDraft} onReset={() => setDraft(effective)} onSave={async next => { await save(next); setEffective(next); setDraft(next); }} />;
  }
  render(<Harness />);
  fireEvent.change(screen.getByLabelText('采样间隔'), { target: { value: '5' } });
  fireEvent.change(screen.getByLabelText('关闭主窗口时'), { target: { value: 'exit' } });
  fireEvent.click(screen.getByLabelText('登录启动时隐藏到托盘'));
  expect((screen.getByLabelText('持续高负载提醒') as HTMLInputElement).checked).toBe(false);
  fireEvent.click(screen.getByLabelText('持续高负载提醒'));
  await act(async () => { fireEvent.click(screen.getByRole('button', { name: '保存设置' })); });
  expect(screen.getByRole('alert').textContent).toContain('无法保存设置');
  expect(save.mock.calls[0][0]).toMatchObject({ samplingIntervalSecs: 5, closeBehavior: 'exit', hideOnAutostart: false, highLoadAlertEnabled: true });
  expect((screen.getByLabelText('采样间隔') as HTMLSelectElement).value).toBe('5');
  expect(screen.getByText(/当前生效：1 秒/)).toBeTruthy();
  await act(async () => { fireEvent.click(screen.getByRole('button', { name: '保存设置' })); });
  expect(screen.getByText('设置已保存并生效')).toBeTruthy();
});
