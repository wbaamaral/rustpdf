// SPDX-License-Identifier: AGPL-3.0-or-later
//! Tarefas em segundo plano: uma thread produz resultados, quem a criou os
//! recolhe sem bloquear (ex.: num temporizador da interface) e pode cancelar.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::Arc;

/// Tarefa em execução. Descartá-la cancela o trabalhador.
pub struct Job<T> {
    receiver: Receiver<T>,
    cancel: Arc<AtomicBool>,
}

/// Lado da thread: entrega resultados e consulta o cancelamento.
pub struct Worker<T> {
    sender: Sender<T>,
    cancel: Arc<AtomicBool>,
}

/// Resultados recolhidos numa consulta; `finished` quando a thread acabou.
pub struct Progress<T> {
    pub items: Vec<T>,
    pub finished: bool,
}

impl<T: Send + 'static> Job<T> {
    pub fn spawn(work: impl FnOnce(&Worker<T>) + Send + 'static) -> Self {
        let (sender, receiver) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker = Worker {
            sender,
            cancel: Arc::clone(&cancel),
        };
        std::thread::spawn(move || work(&worker));
        Self { receiver, cancel }
    }
}

impl<T> Job<T> {
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }

    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }

    /// Tudo o que já chegou, sem bloquear.
    pub fn poll(&self) -> Progress<T> {
        let mut items = Vec::new();
        loop {
            match self.receiver.try_recv() {
                Ok(item) => items.push(item),
                Err(TryRecvError::Empty) => {
                    return Progress {
                        items,
                        finished: false,
                    }
                }
                Err(TryRecvError::Disconnected) => {
                    return Progress {
                        items,
                        finished: true,
                    }
                }
            }
        }
    }
}

impl<T> Drop for Job<T> {
    fn drop(&mut self) {
        self.cancel();
    }
}

impl<T> Worker<T> {
    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }

    /// Entrega um resultado; `false` se a tarefa foi cancelada ou descartada.
    pub fn send(&self, item: T) -> bool {
        !self.cancelled() && self.sender.send(item).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// Recolhe até a tarefa terminar (ou estourar o prazo).
    fn collect<T>(job: &Job<T>) -> Vec<T> {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut all = Vec::new();
        loop {
            let progress = job.poll();
            all.extend(progress.items);
            if progress.finished || Instant::now() > deadline {
                return all;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn delivers_every_item_then_finishes() {
        let job = Job::spawn(|worker| {
            for value in 1..=3 {
                assert!(worker.send(value));
            }
        });
        assert_eq!(collect(&job), [1, 2, 3]);
        assert!(job.poll().finished);
    }

    #[test]
    fn cancel_stops_the_worker() {
        let job = Job::spawn(|worker| {
            let mut value = 0u64;
            while worker.send(value) {
                value += 1;
                std::thread::sleep(Duration::from_millis(1));
            }
            assert!(worker.cancelled());
        });
        std::thread::sleep(Duration::from_millis(20));
        assert!(!job.cancelled());
        job.cancel();
        assert!(job.cancelled());
        collect(&job);
        assert!(job.poll().finished, "o trabalhador parou");
    }

    #[test]
    fn dropping_the_job_cancels_it() {
        let (seen_tx, seen_rx) = std::sync::mpsc::channel();
        let job = Job::spawn(move |worker: &Worker<u8>| {
            while !worker.cancelled() {
                std::thread::sleep(Duration::from_millis(1));
            }
            seen_tx.send(()).unwrap();
        });
        drop(job);
        assert!(seen_rx.recv_timeout(Duration::from_secs(5)).is_ok());
    }
}
