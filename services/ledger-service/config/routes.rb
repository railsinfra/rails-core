Rails.application.routes.draw do
  # Define your application routes per the DSL in https://guides.rubyonrails.org/routing.html

  # Reveal health status on /up that returns 200 if the app boots with no exceptions, otherwise 500.
  # Can be used by load balancers and uptime monitors to verify that the app is live.
  get "up" => "rails/health#show", as: :rails_health_check

  # Health check
  get '/health', to: 'application#health'

  # REST API endpoints for viewing ledger data (read-only)
  namespace :api do
    namespace :v1 do
      namespace :ledger do
        resources :entries, only: [:index]
        resources :transactions, only: [:index, :show]
      end
    end
  end

  # Defines the root path route ("/")
  # root "posts#index"
end
