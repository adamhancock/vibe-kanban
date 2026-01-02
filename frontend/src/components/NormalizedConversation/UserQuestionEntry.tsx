import { useCallback, useEffect, useMemo, useState } from 'react';
import type { ReactNode } from 'react';
import type {
  QuestionAnswer,
  ToolStatus,
  UserQuestion,
  UserQuestionResponse,
} from 'shared/types';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group';
import { Checkbox } from '@/components/ui/checkbox';
import { userQuestionsApi } from '@/lib/api';
import { cn } from '@/lib/utils';

interface UserQuestionEntryProps {
  pendingStatus: Extract<ToolStatus, { status: 'pending_question' }>;
  executionProcessId?: string;
  children: ReactNode;
}

function useQuestionCountdown(
  requestedAt: string | number | Date,
  timeoutAt: string | number | Date,
  paused: boolean
) {
  const totalSeconds = useMemo(() => {
    const total = Math.floor(
      (new Date(timeoutAt).getTime() - new Date(requestedAt).getTime()) / 1000
    );
    return Math.max(1, total);
  }, [requestedAt, timeoutAt]);

  const [timeLeft, setTimeLeft] = useState<number>(() => {
    const remaining = new Date(timeoutAt).getTime() - Date.now();
    return Math.max(0, Math.floor(remaining / 1000));
  });

  useEffect(() => {
    if (paused) return;
    const id = window.setInterval(() => {
      const remaining = new Date(timeoutAt).getTime() - Date.now();
      const next = Math.max(0, Math.floor(remaining / 1000));
      setTimeLeft(next);
      if (next <= 0) window.clearInterval(id);
    }, 1000);

    return () => window.clearInterval(id);
  }, [timeoutAt, paused]);

  const percent = useMemo(
    () =>
      Math.max(0, Math.min(100, Math.round((timeLeft / totalSeconds) * 100))),
    [timeLeft, totalSeconds]
  );

  return { timeLeft, percent };
}

interface QuestionFormProps {
  question: UserQuestion;
  questionIndex: number;
  answer: QuestionAnswer;
  onAnswerChange: (questionIndex: number, answer: QuestionAnswer) => void;
  disabled: boolean;
}

function QuestionForm({
  question,
  questionIndex,
  answer,
  onAnswerChange,
  disabled,
}: QuestionFormProps) {
  const hasOtherSelected =
    answer.custom_text !== undefined && answer.custom_text !== null;

  const handleOptionChange = (optionIndex: number, checked: boolean) => {
    let newSelectedOptions: number[];
    if (question.multiSelect) {
      if (checked) {
        newSelectedOptions = [...answer.selected_options, optionIndex].sort();
      } else {
        newSelectedOptions = answer.selected_options.filter(
          (i) => i !== optionIndex
        );
      }
    } else {
      newSelectedOptions = checked ? [optionIndex] : [];
      // Clear custom text when selecting a predefined option
      if (checked && hasOtherSelected) {
        onAnswerChange(questionIndex, {
          ...answer,
          selected_options: newSelectedOptions,
          custom_text: undefined,
        });
        return;
      }
    }
    onAnswerChange(questionIndex, {
      ...answer,
      selected_options: newSelectedOptions,
    });
  };

  const handleOtherToggle = (checked: boolean) => {
    if (checked) {
      onAnswerChange(questionIndex, {
        ...answer,
        selected_options: question.multiSelect ? answer.selected_options : [],
        custom_text: '',
      });
    } else {
      onAnswerChange(questionIndex, {
        ...answer,
        custom_text: undefined,
      });
    }
  };

  const handleCustomTextChange = (text: string) => {
    onAnswerChange(questionIndex, {
      ...answer,
      custom_text: text,
    });
  };

  return (
    <div className="space-y-3">
      {question.header && (
        <div className="text-muted-foreground text-xs font-medium uppercase tracking-wide">
          {question.header}
        </div>
      )}
      <div className="font-medium">{question.question}</div>

      <div className="space-y-2">
        {question.multiSelect ? (
          // Multi-select with checkboxes
          <>
            {question.options.map((option, optionIndex) => (
              <div key={optionIndex} className="flex items-start gap-2">
                <Checkbox
                  id={`q${questionIndex}-opt${optionIndex}`}
                  checked={answer.selected_options.includes(optionIndex)}
                  onCheckedChange={(checked) =>
                    handleOptionChange(optionIndex, checked === true)
                  }
                  disabled={disabled}
                />
                <div className="flex flex-col">
                  <Label
                    htmlFor={`q${questionIndex}-opt${optionIndex}`}
                    className="cursor-pointer font-normal"
                  >
                    {option.label}
                  </Label>
                  {option.description && (
                    <span className="text-muted-foreground text-xs">
                      {option.description}
                    </span>
                  )}
                </div>
              </div>
            ))}
            {/* Other option */}
            <div className="flex items-start gap-2">
              <Checkbox
                id={`q${questionIndex}-other`}
                checked={hasOtherSelected}
                onCheckedChange={(checked) =>
                  handleOtherToggle(checked === true)
                }
                disabled={disabled}
              />
              <div className="flex flex-1 flex-col gap-1">
                <Label
                  htmlFor={`q${questionIndex}-other`}
                  className="cursor-pointer font-normal"
                >
                  Other
                </Label>
                {hasOtherSelected && (
                  <Input
                    value={answer.custom_text || ''}
                    onChange={(e) => handleCustomTextChange(e.target.value)}
                    placeholder="Enter your answer..."
                    disabled={disabled}
                    className="h-8"
                  />
                )}
              </div>
            </div>
          </>
        ) : (
          // Single-select with radio buttons
          <RadioGroup
            value={
              hasOtherSelected
                ? 'other'
                : answer.selected_options.length > 0
                  ? String(answer.selected_options[0])
                  : undefined
            }
            onValueChange={(value: string) => {
              if (value === 'other') {
                handleOtherToggle(true);
              } else {
                const optionIndex = parseInt(value, 10);
                handleOptionChange(optionIndex, true);
              }
            }}
            disabled={disabled}
          >
            {question.options.map((option, optionIndex) => (
              <div key={optionIndex} className="flex items-start gap-2">
                <RadioGroupItem
                  value={String(optionIndex)}
                  id={`q${questionIndex}-opt${optionIndex}`}
                />
                <div className="flex flex-col">
                  <Label
                    htmlFor={`q${questionIndex}-opt${optionIndex}`}
                    className="cursor-pointer font-normal"
                  >
                    {option.label}
                  </Label>
                  {option.description && (
                    <span className="text-muted-foreground text-xs">
                      {option.description}
                    </span>
                  )}
                </div>
              </div>
            ))}
            {/* Other option */}
            <div className="flex items-start gap-2">
              <RadioGroupItem
                value="other"
                id={`q${questionIndex}-other`}
              />
              <div className="flex flex-1 flex-col gap-1">
                <Label
                  htmlFor={`q${questionIndex}-other`}
                  className="cursor-pointer font-normal"
                >
                  Other
                </Label>
                {hasOtherSelected && (
                  <Input
                    value={answer.custom_text || ''}
                    onChange={(e) => handleCustomTextChange(e.target.value)}
                    placeholder="Enter your answer..."
                    disabled={disabled}
                    className="h-8"
                  />
                )}
              </div>
            </div>
          </RadioGroup>
        )}
      </div>
    </div>
  );
}

const UserQuestionEntry = ({
  pendingStatus,
  executionProcessId,
  children,
}: UserQuestionEntryProps) => {
  const [isResponding, setIsResponding] = useState(false);
  const [hasResponded, setHasResponded] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Initialize answers for all questions
  const [answers, setAnswers] = useState<QuestionAnswer[]>(() =>
    pendingStatus.questions.map((_, index) => ({
      question_index: index,
      selected_options: [],
      custom_text: undefined,
    }))
  );

  const { timeLeft } = useQuestionCountdown(
    pendingStatus.requested_at,
    pendingStatus.timeout_at,
    hasResponded
  );

  const disabled = isResponding || hasResponded || timeLeft <= 0;

  // Check if all questions have been answered
  const allQuestionsAnswered = useMemo(() => {
    return answers.every((answer) => {
      const hasSelection = answer.selected_options.length > 0;
      const hasCustomText =
        answer.custom_text !== undefined && answer.custom_text.trim() !== '';
      // For single-select, need exactly one selection OR custom text
      // For multi-select, need at least one selection OR custom text
      return hasSelection || hasCustomText;
    });
  }, [answers]);

  const handleAnswerChange = useCallback(
    (questionIndex: number, answer: QuestionAnswer) => {
      setAnswers((prev) => {
        const next = [...prev];
        next[questionIndex] = answer;
        return next;
      });
    },
    []
  );

  const handleSubmit = useCallback(async () => {
    if (disabled || !allQuestionsAnswered) return;
    if (!executionProcessId) {
      setError('Missing executionProcessId');
      return;
    }

    setIsResponding(true);
    setError(null);

    const response: UserQuestionResponse = {
      execution_process_id: executionProcessId,
      answers,
    };

    try {
      await userQuestionsApi.respond(pendingStatus.question_id, response);
      setHasResponded(true);
    } catch (e: unknown) {
      console.error('Question respond failed:', e);
      const errorMessage =
        e instanceof Error ? e.message : 'Failed to send response';
      setError(errorMessage);
    } finally {
      setIsResponding(false);
    }
  }, [
    disabled,
    allQuestionsAnswered,
    executionProcessId,
    answers,
    pendingStatus.question_id,
  ]);

  return (
    <div className="relative mt-3">
      <div className="overflow-hidden">
        {children}

        <div className="bg-background px-4 py-3">
          {pendingStatus.questions.map((question, index) => (
            <div
              key={index}
              className={cn(
                'py-3',
                index > 0 && 'border-border border-t'
              )}
            >
              <QuestionForm
                question={question}
                questionIndex={index}
                answer={answers[index]}
                onAnswerChange={handleAnswerChange}
                disabled={disabled}
              />
            </div>
          ))}

          {error && (
            <div
              className="mt-2 text-xs text-red-600"
              role="alert"
              aria-live="polite"
            >
              {error}
            </div>
          )}

          <div className="mt-3 flex items-center justify-between">
            <div className="text-muted-foreground text-xs">
              {timeLeft > 0
                ? `${Math.floor(timeLeft / 60)}:${String(timeLeft % 60).padStart(2, '0')} remaining`
                : 'Timed out'}
            </div>
            <Button
              onClick={handleSubmit}
              disabled={disabled || !allQuestionsAnswered}
              size="sm"
            >
              {isResponding ? 'Submitting...' : 'Submit'}
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
};

export default UserQuestionEntry;
