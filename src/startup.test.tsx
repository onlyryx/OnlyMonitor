import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { useState } from 'react';
import { StartupPanel } from './StartupPanel';
import { SettingsPanel } from './SettingsPanel';
import { monitorState } from './testFixtures';
import * as api from './api';

afterEach(() => { cleanup(); vi.restoreAllMocks(); });

it('shows unknown read failures and preserves actual state on failed change without submitting drafts', async () => {
  vi.spyOn(api, 'getStartupStatus').mockResolvedValueOnce({ registered: null, error: '无法读取' })
    .mockResolvedValue({ registered: false, error: null });
  vi.spyOn(api, 'listenVisibility').mockResolvedValue(() => {});
  const change = vi.fn().mockResolvedValue({ registered: false, error: '拒绝写入' });
  const save = vi.fn();
  const settings = monitorState().settings;
  function SettingsHarness() {
    const [draft, setDraft] = useState(settings);
    return <SettingsPanel settings={settings} draft={draft} onDraftChange={setDraft} onReset={() => setDraft(settings)} onSave={save} />;
  }
  await act(async () => { render(<><StartupPanel onChange={change} disabled={false} /><SettingsHarness /></>); });
  expect(screen.getByText('无法读取登记状态')).toBeTruthy();
  fireEvent.change(screen.getByLabelText('采样间隔'), { target: { value: '5' } });
  await act(async () => { fireEvent.click(screen.getByRole('button', { name: '刷新启动状态' })); });
  expect(screen.getByText('未注册')).toBeTruthy();
  await act(async () => { fireEvent.click(screen.getByRole('button', { name: '启用登录启动' })); });
  expect(screen.getByText('未注册')).toBeTruthy();
  expect(screen.getByText('拒绝写入')).toBeTruthy();
  expect(save).not.toHaveBeenCalled();
  expect((screen.getByLabelText('采样间隔') as HTMLSelectElement).value).toBe('5');
});

it('refreshes on restore, ignores stale reads, and cleans listeners on unmount', async () => {
  let visibility!: (visible: boolean) => void;
  const off = vi.fn();
  vi.spyOn(api, 'listenVisibility').mockImplementation(async handler => { visibility = handler; return off; });
  let resolveRead!: (value: { registered: boolean; error: null }) => void;
  const read = vi.spyOn(api, 'getStartupStatus').mockImplementationOnce(() => new Promise(r => { resolveRead = r; }))
    .mockResolvedValue({ registered: false, error: null });
  const change = vi.fn().mockResolvedValue({ registered: true, error: null });
  let view!: ReturnType<typeof render>;
  await act(async () => { view = render(<StartupPanel onChange={change} disabled={false} />); });
  await act(async () => { visibility(true); });
  expect(read).toHaveBeenCalledTimes(2);
  await act(async () => { fireEvent.click(screen.getByRole('button', { name: '启用登录启动' })); });
  await act(async () => { resolveRead({ registered: false, error: null }); });
  expect(screen.getByText('已注册')).toBeTruthy();
  view.unmount();
  expect(off).toHaveBeenCalledTimes(1);
});
