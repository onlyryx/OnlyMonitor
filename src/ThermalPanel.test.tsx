import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { ThermalPanel } from './ThermalPanel';
import { readThermalZones } from './api';

vi.mock('./api', () => ({ readThermalZones: vi.fn() }));
afterEach(() => { cleanup(); vi.resetAllMocks(); });

it('shows a real thermal reading and reports unavailable sensors without inventing a value', async () => {
  vi.mocked(readThermalZones).mockResolvedValueOnce({ sampledAtMs: 1000, readings: [{ label: 'Computer', celsius: 42 }] });
  render(<ThermalPanel />);
  fireEvent.click(screen.getByRole('button', { name: '读取系统热区' }));
  expect(await screen.findByText('42.0 °C')).toBeTruthy();
  vi.mocked(readThermalZones).mockRejectedValueOnce(new Error('系统未提供可读取的温度热区'));
  fireEvent.click(screen.getByRole('button', { name: '读取系统热区' }));
  await waitFor(() => expect(screen.getByRole('alert').textContent).toContain('系统未提供'));
  expect(screen.queryByText('42.0 °C')).toBeNull();
});
