import { Check, ChevronDown } from 'lucide-react';
import { useEffect, useId, useRef, useState, type KeyboardEvent, type ReactNode } from 'react';

import { useDismissibleLayer } from './useDismissibleLayer';
import styles from './SelectMenu.module.css';

export interface SelectMenuOption<Value extends string = string> {
  icon?: ReactNode;
  label: string;
  value: Value;
}

interface SelectMenuProps<Value extends string> {
  ariaLabel: string;
  className?: string | undefined;
  compact?: boolean;
  defaultValue?: Value | undefined;
  disabled?: boolean;
  name?: string;
  onValueChange?: ((value: Value) => void) | undefined;
  options: readonly SelectMenuOption<Value>[];
  value?: Value | undefined;
}

/** Product-styled single-value listbox with keyboard and light-dismiss behavior. */
export function SelectMenu<Value extends string>({
  ariaLabel,
  className,
  compact = false,
  defaultValue,
  disabled = false,
  name,
  onValueChange,
  options,
  value,
}: SelectMenuProps<Value>) {
  const [open, setOpen] = useState(false);
  const [internalValue, setInternalValue] = useState(defaultValue ?? options[0]?.value);
  const id = useId();
  const triggerRef = useRef<HTMLButtonElement>(null);
  const listboxRef = useRef<HTMLUListElement>(null);
  const optionByValueRef = useRef(new Map<Value, HTMLButtonElement>());
  const selectedValue = value ?? internalValue;
  const selected = options.find((option) => option.value === selectedValue) ?? options[0];

  useDismissibleLayer({
    active: open,
    layerRef: listboxRef,
    onDismiss: () => setOpen(false),
    triggerRef,
  });

  useEffect(() => {
    if (open && selectedValue !== undefined) optionByValueRef.current.get(selectedValue)?.focus();
  }, [open, selectedValue]);

  const moveFocus = (direction: 1 | -1) => {
    const activeValue =
      document.activeElement instanceof HTMLElement
        ? document.activeElement.dataset.value
        : undefined;
    const current = options.findIndex((option) => option.value === activeValue);
    const selectedIndex = options.findIndex((option) => option.value === selectedValue);
    const start = current < 0 ? selectedIndex : current;
    const next = (start + direction + options.length) % options.length;
    const nextOption = options[next];
    if (nextOption !== undefined) optionByValueRef.current.get(nextOption.value)?.focus();
  };

  const handleTriggerKeyDown = (event: KeyboardEvent<HTMLButtonElement>) => {
    if (event.key !== 'ArrowDown' && event.key !== 'ArrowUp') return;
    event.preventDefault();
    setOpen(true);
  };

  const handleOptionKeyDown = (event: KeyboardEvent<HTMLButtonElement>) => {
    if (event.key === 'Escape') {
      event.preventDefault();
      setOpen(false);
      triggerRef.current?.focus();
    } else if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault();
      moveFocus(event.key === 'ArrowDown' ? 1 : -1);
    } else if (event.key === 'Home' || event.key === 'End') {
      event.preventDefault();
      const option = event.key === 'Home' ? options[0] : options.at(-1);
      if (option !== undefined) optionByValueRef.current.get(option.value)?.focus();
    }
  };

  return (
    <div
      className={`${styles.root}${compact ? ` ${styles.compact}` : ''}${className === undefined ? '' : ` ${className}`}`}
    >
      {name === undefined ? null : <input name={name} type="hidden" value={selectedValue ?? ''} />}
      <button
        aria-controls={`${id}-listbox`}
        aria-expanded={open}
        aria-haspopup="listbox"
        aria-label={ariaLabel}
        className={styles.trigger}
        disabled={disabled}
        onClick={() => setOpen((current) => !current)}
        onKeyDown={handleTriggerKeyDown}
        ref={triggerRef}
        role="combobox"
        type="button"
      >
        <span className={styles.value}>
          {selected?.icon}
          <span>{selected?.label}</span>
        </span>
        <ChevronDown aria-hidden="true" className={styles.chevron} size={15} />
      </button>
      {open ? (
        <ul
          aria-label={ariaLabel}
          className={styles.listbox}
          id={`${id}-listbox`}
          ref={listboxRef}
          role="listbox"
        >
          {options.map((option) => (
            <li key={option.value}>
              <button
                aria-selected={option.value === selectedValue}
                className={styles.option}
                data-value={option.value}
                onClick={() => {
                  setInternalValue(option.value);
                  onValueChange?.(option.value);
                  setOpen(false);
                  triggerRef.current?.focus();
                }}
                onKeyDown={handleOptionKeyDown}
                ref={(element) => {
                  if (element === null) optionByValueRef.current.delete(option.value);
                  else optionByValueRef.current.set(option.value, element);
                }}
                role="option"
                type="button"
              >
                <span className={styles.optionLabel}>
                  {option.icon}
                  <span>{option.label}</span>
                </span>
                {option.value === selectedValue ? <Check aria-hidden="true" size={16} /> : null}
              </button>
            </li>
          ))}
        </ul>
      ) : null}
    </div>
  );
}
